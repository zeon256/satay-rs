use std::fs;

use crate::ast::*;
use crate::common::*;

#[test]
fn simple_fixture_generates_expected_file_structure() {
    let files = satay_codegen::generate(SIMPLE).expect("generate simple fixture");

    let mod_rs = parse_rust(find_file(&files, "mod.rs"));
    assert!(is_pub(&find_const(&mod_rs, "SERVER_URL").vis));
    assert!(is_pub(&find_mod(&mod_rs, "types").vis));
    let api_mod = find_mod(&mod_rs, "api");
    assert!(!is_pub(&api_mod.vis));
    assert!(has_cfg_feature(&api_mod.attrs, "json"));
    assert!(is_pub(&find_mod(&mod_rs, "get_user").vis));
    assert!(is_pub(&find_mod(&mod_rs, "update_user").vis));

    assert!(!files.iter().any(|file| file.relative_path == "lib.rs"));

    assert!(!files.iter().any(|file| file.relative_path == "client.rs"));
    let api_rs = parse_rust(find_file(&files, "api.rs"));
    find_struct(&api_rs, "Api");
    find_struct(&api_rs, "GetUserAction");
    find_method(&api_rs, "Api", "get_user");
    find_method(&api_rs, "GetUserAction", "request");
    find_method(&api_rs, "GetUserAction", "decode");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    find_struct(&types_rs, "ErrorBody");
    find_struct(&types_rs, "UpdateUserRequest");
    find_struct(&types_rs, "User");
    find_enum(&types_rs, "UserStatus");

    let parts = parse_rust(find_file(&files, "get_user/parts.rs"));
    find_struct(&parts, "GetUserInput");
    let new_fn = find_method(&parts, "GetUserInput", "new");
    assert!(is_pub(&new_fn.vis));
    assert_eq!(
        norm(&new_fn.sig),
        norm_str("fn new(user_id: impl Into<String>) -> Self")
    );
    find_method(&parts, "GetUserInput", "include_details");
    find_enum(&parts, "GetUserResponse");
    find_fn(&parts, "get_user_parts");
    assert!(!contains_tokens(&parts, r#"#[cfg(feature = "json")]"#));

    let json = parse_rust(find_file(&files, "get_user/json.rs"));
    find_fn(&json, "encode_get_user");
    find_fn(&json, "decode_get_user_response");
    assert!(!contains_tokens(&json, r#"#[cfg(feature = "json")]"#));

    let parts = parse_rust(find_file(&files, "update_user/parts.rs"));
    find_struct(&parts, "UpdateUserInput");
    find_enum(&parts, "UpdateUserResponse");
    find_fn(&parts, "update_user_parts");

    let json = parse_rust(find_file(&files, "update_user/json.rs"));
    find_fn(&json, "encode_update_user");
    find_fn(&json, "decode_update_user_response");
}

#[test]
fn lib_root_module_option_emits_lib_rs_instead_of_mod_rs() {
    use satay_codegen::{GenerateOptions, RootModule};
    let options = GenerateOptions {
        root_module: RootModule::LibRs,
    };
    let files = satay_codegen::generate_with(SIMPLE, options).expect("generate simple fixture");

    let lib_rs = parse_rust(find_file(&files, "lib.rs"));
    assert!(is_pub(&find_const(&lib_rs, "SERVER_URL").vis));
    assert!(!files.iter().any(|file| file.relative_path == "mod.rs"));
    assert!(
        files
            .iter()
            .any(|file| file.relative_path == "get_user/mod.rs")
    );
}

#[test]
fn descriptions_generate_rustdoc_comments() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /users/{userId}:
    get:
      operationId: getUser
      description: Fetch a user.
      parameters:
        - name: userId
          in: path
          required: true
          description: User identifier.
          schema:
            type: string
        - name: includeDetails
          in: query
          description: Include detailed fields.
          schema:
            type: boolean
      responses:
        '200':
          description: User found.
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
components:
  schemas:
    User:
      type: object
      description: A user record.
      required:
        - id
        - code
        - home_code
      properties:
        id:
          type: string
          description: Stable ID.
        name:
          type: string
          description: Display name.
        code:
          $ref: '#/components/schemas/UserCode'
        home_code:
          $ref: '#/components/schemas/UserCode'
          description: Home user code.
    UserCode:
      type: string
      description: Reusable user code.
      maxLength: 32
"#,
    )
    .expect("generate descriptions fixture");

    let api_rs = parse_rust(find_file(&files, "api.rs"));
    assert_doc(
        &find_method(&api_rs, "Api", "get_user").attrs,
        "Fetch a user.",
    );

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let user = find_struct(&types_rs, "User");
    assert_doc(&user.attrs, "A user record.");
    assert_doc(&field(user, "id").attrs, "Stable ID.");
    assert_field(user, "id", "String");
    assert_doc(&field(user, "name").attrs, "Display name.");
    assert_field(user, "name", "Option<String>");
    assert_doc(&field(user, "code").attrs, "Reusable user code.");
    assert_field(user, "code", "UserCode");
    assert_doc(&field(user, "home_code").attrs, "Home user code.");
    assert_field(user, "home_code", "UserCode");

    let parts_rs = parse_rust(find_file(&files, "get_user/parts.rs"));
    let input = find_struct(&parts_rs, "GetUserInput");
    assert_doc(&input.attrs, "Fetch a user.");
    assert_doc(&field(input, "user_id").attrs, "User identifier.");
    assert_field(input, "user_id", "String");
    assert_doc(
        &field(input, "include_details").attrs,
        "Include detailed fields.",
    );
    let response = find_enum(&parts_rs, "GetUserResponse");
    let ok = variant(response, "Ok");
    assert_doc(&ok.attrs, "User found.");
    assert_eq!(norm(&ok.fields), norm_str("(User)"));
}

#[test]
fn server_security_and_api_action_helpers_are_generated() {
    let files = satay_codegen::generate(
        r#"
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
        '200':
          description: Found
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
components:
  securitySchemes:
    accountKeyAuth:
      type: apiKey
      in: header
      name: AccountKey
    apiKeyQuery:
      type: apiKey
      in: query
      name: api_key
  schemas:
    User:
      type: object
      required:
        - id
      properties:
        id:
          type: string
"#,
    )
    .expect("generate secured fixture");

    let mod_rs = parse_rust(find_file(&files, "mod.rs"));
    assert_eq!(
        norm(find_const(&mod_rs, "SERVER_URL")),
        norm_str(r#"pub const SERVER_URL: &str = "https://api.example.test/v1";"#)
    );

    let api_rs = parse_rust(find_file(&files, "api.rs"));
    find_method(&api_rs, "Api", "account_key");
    find_method(&api_rs, "Api", "api_key");
    find_method(&api_rs, "Api", "get_user");
    find_struct(&api_rs, "GetUserAction");
    assert!(contains_tokens(&api_rs, "satay_runtime::insert_header"));
    assert!(contains_tokens(&api_rs, r#""AccountKey""#));
    assert!(contains_tokens(&api_rs, "satay_runtime::append_query_pair"));
    assert!(contains_tokens(&api_rs, r#""api_key""#));
    assert!(contains_tokens(&api_rs, "parts.uri = format!"));
    assert!(!contains_ident(&api_rs, "reqwest"));

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
    fn action_applies_base_url_and_api_keys() {
        let request = Api::new()
            .account_key("secret")
            .api_key("query secret")
            .get_user("42")
            .request()
            .expect("action request");

        assert_eq!(
            request.uri().to_string(),
            "https://api.example.test/v1/users/42?api_key=query%20secret"
        );
        let account_key = http::header::HeaderName::from_bytes(b"AccountKey").unwrap();
        assert_eq!(request.headers().get(account_key).unwrap(), "secret");
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "secured generated crate tests");
}

#[test]
fn simple_fixture_endpoint_modules_have_cfg_gated_json() {
    let files = satay_codegen::generate(SIMPLE).expect("generate simple fixture");

    let mod_rs = parse_rust(find_file(&files, "get_user/mod.rs"));
    assert!(has_cfg_feature(&find_mod(&mod_rs, "json").attrs, "json"));
    assert!(contains_tokens(
        &mod_rs,
        r#"#[cfg(feature = "json")] pub use json::*;"#
    ));

    let mod_rs = parse_rust(find_file(&files, "update_user/mod.rs"));
    assert!(has_cfg_feature(&find_mod(&mod_rs, "json").attrs, "json"));
    assert!(contains_tokens(
        &mod_rs,
        r#"#[cfg(feature = "json")] pub use json::*;"#
    ));

    let parts = parse_rust(find_file(&files, "get_user/parts.rs"));
    assert!(!contains_tokens(&parts, r#"#[cfg(feature = "json")]"#));

    let json = parse_rust(find_file(&files, "get_user/json.rs"));
    assert!(!contains_tokens(&json, r#"#[cfg(feature = "json")]"#));
}

#[test]
fn petstore_minimal_fixture_generates_client_core() {
    let files = satay_codegen::generate(PETSTORE_MINIMAL).expect("generate petstore fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    find_struct(&types_rs, "Pet");

    let parts = parse_rust(find_file(&files, "list_pets/parts.rs"));
    find_fn(&parts, "list_pets_parts");

    let json = parse_rust(find_file(&files, "create_pet/json.rs"));
    find_fn(&json, "encode_create_pet");

    let json = parse_rust(find_file(&files, "get_pet/json.rs"));
    find_fn(&json, "decode_get_pet_response");
}
