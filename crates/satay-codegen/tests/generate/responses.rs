use super::codegen;
use std::fs;

use syn::Fields;

use super::ast::*;
use super::common::*;

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
    let files = codegen::generate(WILDCARD_RESPONSES).expect("generate wildcard fixture");

    let parts = parse_rust(find_file(&files, "get_user/parts.rs"));
    let response = find_enum(&parts, "GetUserResponse");
    assert_eq!(
        norm_fields(&variant(response, "ClientError").fields),
        norm_str("(http::StatusCode, ErrorResponse<'storage, S>)")
    );
    assert_eq!(
        norm_fields(&variant(response, "Ok").fields),
        norm_str("(User<'storage, S>)")
    );
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
    let files = codegen::generate(WILDCARD_RESPONSES).expect("generate wildcard fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/responses/generated_wildcard_range_decodes_with_exact_status_precedence/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
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
    let files = codegen::generate(PROJECTED_RESPONSES).expect("generate projection fixture");

    let services_parts = parse_rust(find_file(&files, "get_services/parts.rs"));
    let services_response = find_enum(&services_parts, "GetServicesResponse");
    assert_eq!(
        norm_fields(&variant(services_response, "Ok").fields),
        norm_str(
            "(<S as satay_runtime::storage::Storage>::Contiguous<'storage, Service<'storage, S>>)"
        )
    );
    let services_json = parse_rust(find_file(&files, "get_services/json.rs"));
    let services_decode = norm(find_fn(&services_json, "decode_get_services_response"));
    assert!(
        services_decode.contains(&norm_str(
            "satay_runtime::from_projected_json_slice::<<S as satay_runtime::storage::Storage>::Contiguous<'storage, Service<'storage, S>>,>(body, \"value\", None)?"
        )),
        "{services_decode}"
    );

    let links_parts = parse_rust(find_file(&files, "get_links/parts.rs"));
    let links_response = find_enum(&links_parts, "GetLinksResponse");
    assert_eq!(
        norm_fields(&variant(links_response, "Ok").fields),
        norm_str(
            "(<S as satay_runtime::storage::Storage>::Contiguous<'storage, <S as satay_runtime::storage::Storage>::Text<'storage>>)"
        )
    );
    let links_json = parse_rust(find_file(&files, "get_links/json.rs"));
    let links_decode = norm(find_fn(&links_json, "decode_get_links_response"));
    assert!(links_decode.contains(&norm_str(
        "satay_runtime::from_projected_json_slice::<<S as satay_runtime::storage::Storage>::Contiguous<'storage, <S as satay_runtime::storage::Storage>::Text<'storage>>,>(body, \"value\", Some(\"Link\"))?"
    )));
}

#[test]
fn generated_response_projection_decodes_wire_wrappers() {
    let files = codegen::generate(PROJECTED_RESPONSES).expect("generate projection fixture");
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    write_manifest(crate_dir, &runtime_path_toml(), false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/responses/generated_response_projection_decodes_wire_wrappers/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "response projection generated crate tests",
    );
}

#[test]
fn optional_projected_fields_preserve_public_types_and_decoding() {
    let spec = PROJECTED_RESPONSES
        .replace("required: [value]", "required: []")
        .replace("required: [Link]", "required: []");
    let files = codegen::generate(&spec).unwrap();
    let temp = tempfile::tempdir().unwrap();
    write_manifest(temp.path(), &runtime_path_toml(), false, false);
    write_generated_files(&temp.path().join("src/generated"), &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/responses/optional_projected_fields_preserve_public_types_and_decoding/tests.rs"
        ),
    );
    fs::write(temp.path().join("src/lib.rs"), TEST_CRATE_LIB).unwrap();
    run_temp_cargo(temp.path(), "test", &[], "optional response projections");
}
