use std::fs;

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
    assert!(matches!(
        variant(response, "NotFound").fields,
        syn::Fields::Unit
    ));

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
        let decoded = decode_get_user_response(response).expect("decoded response");

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
        let decoded = decode_get_user_response(response).expect("decoded response");

        assert!(matches!(decoded, GetUserResponse::NotFound));
    }

    #[test]
    fn statuses_outside_declared_ranges_stay_unexpected() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::INTERNAL_SERVER_ERROR,
            headers: http::HeaderMap::new(),
            body: b"boom".to_vec(),
        };
        let decoded = decode_get_user_response(response).expect("decoded response");

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
