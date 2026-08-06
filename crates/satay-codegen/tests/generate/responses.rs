use std::fs;

use syn::Fields;

use crate::ast::*;
use crate::common::*;

const WILDCARD_RESPONSES: &str = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
servers:
  - url: https://api.example.test/v1
paths:
  /users/{userId}:
    get:
      operationId: getUser
      parameters:
        - name: userId
          in: path
          required: true
          schema:
            type: string
      responses:
        '4XX':
          description: Client error
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'
        '200':
          description: Found user
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
        '404':
          description: Not found
components:
  schemas:
    User:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    ErrorResponse:
      type: object
      required:
        - message
      properties:
        message:
          type: string
"#;

const PROJECTED_RESPONSES: &str = r#"
openapi: 3.1.0
info:
  title: Projected responses
  version: 1.0.0
paths:
  /services:
    get:
      operationId: getServices
      x-satay:
        output:
          unwrap-field: value
      responses:
        '200':
          description: Services
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ServiceEnvelope'
  /links:
    get:
      operationId: getLinks
      x-satay:
        output:
          unwrap-field: value
          map-field: Link
      responses:
        '200':
          description: Links
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/LinkEnvelope'
components:
  schemas:
    ServiceEnvelope:
      type: object
      required: [value]
      properties:
        odata.metadata:
          type: string
        value:
          type: array
          items:
            $ref: '#/components/schemas/Service'
    Service:
      type: object
      required: [id, name]
      properties:
        id:
          type: string
        name:
          type: string
    LinkEnvelope:
      type: object
      required: [value]
      properties:
        value:
          type: array
          items:
            $ref: '#/components/schemas/LinkRow'
    LinkRow:
      type: object
      required: [Link]
      properties:
        Link:
          type: string
        Description:
          type: string
"#;

#[test]
fn wildcard_range_generates_status_carrying_variant_after_exact_arms() {
    let files = satay_codegen::generate(WILDCARD_RESPONSES).expect("generate wildcard fixture");

    let parts = parse_rust(find_file(&files, "get_user/parts.rs"));
    let response = find_enum(&parts, "GetUserResponse");
    assert_eq!(
        norm(&variant(response, "ClientError").fields),
        norm_str("(http::StatusCode, ErrorResponse)")
    );
    assert_eq!(norm(&variant(response, "Ok").fields), norm_str("(User)"));
    assert!(matches!(variant(response, "NotFound").fields, Fields::Unit));

    // Exact-status arms must precede the covering range arm so 404 shadows
    // 400..=499; UnexpectedStatus stays last.
    let json = parse_rust(find_file(&files, "get_user/json.rs"));
    let decode = norm(find_fn(&json, "decode_get_user_response"));
    let ok_arm = decode.find(&norm_str("200 =>")).expect("200 arm");
    let not_found_arm = decode.find(&norm_str("404 =>")).expect("404 arm");
    let range_arm = decode.find(&norm_str("400..=499 =>")).expect("range arm");
    assert!(ok_arm < not_found_arm && not_found_arm < range_arm);
}

#[test]
fn generated_wildcard_range_decodes_with_exact_status_precedence() {
    let files = satay_codegen::generate(WILDCARD_RESPONSES).expect("generate wildcard fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn decodes_range_body_with_concrete_status() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::TOO_MANY_REQUESTS,
            headers: http::HeaderMap::new(),
            body: br#"{"message":"slow down"}"#.to_vec(),
        };
        let decoded = operations::get_user::decode_get_user_response(response)
            .expect("decoded response");

        match decoded {
            GetUserResponse::ClientError(status, error) => {
                assert_eq!(status, http::StatusCode::TOO_MANY_REQUESTS);
                assert_eq!(error.message, "slow down");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn exact_status_shadows_covering_range() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::NOT_FOUND,
            headers: http::HeaderMap::new(),
            body: Vec::new(),
        };
        let decoded = operations::get_user::decode_get_user_response(response)
            .expect("decoded response");

        assert!(matches!(decoded, GetUserResponse::NotFound));
    }

    #[test]
    fn statuses_outside_declared_ranges_stay_unexpected() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::INTERNAL_SERVER_ERROR,
            headers: http::HeaderMap::new(),
            body: b"boom".to_vec(),
        };
        let decoded = operations::get_user::decode_get_user_response(response)
            .expect("decoded response");

        match decoded {
            GetUserResponse::UnexpectedStatus(status, body) => {
                assert_eq!(status, http::StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body, b"boom");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "wildcard range generated crate tests",
    );
}

#[test]
fn response_projection_generates_public_payload_types_and_projected_decoders() {
    let files = satay_codegen::generate(PROJECTED_RESPONSES).expect("generate projection fixture");

    let services_parts = parse_rust(find_file(&files, "get_services/parts.rs"));
    let services_response = find_enum(&services_parts, "GetServicesResponse");
    assert_eq!(
        norm(&variant(services_response, "Ok").fields),
        norm_str("(Vec<Service>)")
    );
    let services_json = parse_rust(find_file(&files, "get_services/json.rs"));
    let services_decode = norm(find_fn(&services_json, "decode_get_services_response"));
    assert!(
        services_decode.contains(&norm_str(
            "satay_runtime::from_projected_json_slice::<Vec<Service>,>(body.as_ref(), \"value\", None)?"
        )),
        "{services_decode}"
    );

    let links_parts = parse_rust(find_file(&files, "get_links/parts.rs"));
    let links_response = find_enum(&links_parts, "GetLinksResponse");
    assert_eq!(
        norm(&variant(links_response, "Ok").fields),
        norm_str("(Vec<String>)")
    );
    let links_json = parse_rust(find_file(&files, "get_links/json.rs"));
    let links_decode = norm(find_fn(&links_json, "decode_get_links_response"));
    assert!(links_decode.contains(&norm_str(
        "satay_runtime::from_projected_json_slice::<Vec<String>,>(body.as_ref(), \"value\", Some(\"Link\"))?"
    )));
}

#[test]
fn generated_response_projection_decodes_wire_wrappers() {
    let files = satay_codegen::generate(PROJECTED_RESPONSES).expect("generate projection fixture");
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    write_manifest(crate_dir, &runtime_path_toml(), false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn unwraps_value_payload() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{
                "odata.metadata":"https://example.test/metadata",
                "value":[
                    {"id":"10","name":"Airport Express"},
                    {"id":"20","name":"City Loop"}
                ]
            }"#.to_vec(),
        };
        let decoded = operations::get_services::decode_get_services_response(response)
            .expect("projected services");

        match decoded {
            GetServicesResponse::Ok(services) => {
                assert_eq!(services.len(), 2);
                assert_eq!(services[0].id, "10");
                assert_eq!(services[1].name, "City Loop");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn unwraps_and_maps_link_payload() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{
                "value":[
                    {"Link":"https://example.test/a","Description":"A"},
                    {"Link":"https://example.test/b","Description":"B"}
                ]
            }"#.to_vec(),
        };
        let decoded = operations::get_links::decode_get_links_response(response)
            .expect("projected links");

        match decoded {
            GetLinksResponse::Ok(links) => assert_eq!(
                links,
                vec![
                    "https://example.test/a".to_owned(),
                    "https://example.test/b".to_owned(),
                ]
            ),
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "response projection generated crate tests",
    );
}
