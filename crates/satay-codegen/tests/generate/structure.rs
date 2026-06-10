use std::fs;

use crate::common::*;

#[test]
fn simple_fixture_generates_expected_file_structure() {
    let files = satay_codegen::generate(SIMPLE).expect("generate simple fixture");

    let mod_rs = find_file(&files, "mod.rs");
    assert!(mod_rs.contents.contains("pub const SERVER_URL"));
    assert!(mod_rs.contents.contains("pub mod types;"));
    assert!(mod_rs.contents.contains("#[cfg(feature = \"json\")]"));
    assert!(mod_rs.contents.contains("mod api;"));
    assert!(mod_rs.contents.contains("pub mod get_user;"));
    assert!(mod_rs.contents.contains("pub mod update_user;"));

    assert!(!files.iter().any(|file| file.relative_path == "lib.rs"));

    assert!(!files.iter().any(|file| file.relative_path == "client.rs"));
    let api_rs = find_file(&files, "api.rs");
    assert!(api_rs.contents.contains("pub struct Api"));
    assert!(api_rs.contents.contains("pub struct GetUserAction"));
    assert!(api_rs.contents.contains("pub fn get_user"));
    assert!(api_rs.contents.contains("pub fn request"));
    assert!(api_rs.contents.contains("pub fn decode"));

    let types_rs = find_file(&files, "types.rs");
    assert!(types_rs.contents.contains("pub struct ErrorBody"));
    assert!(types_rs.contents.contains("pub struct UpdateUserRequest"));
    assert!(types_rs.contents.contains("pub struct User"));
    assert!(types_rs.contents.contains("pub enum UserStatus"));

    let parts = find_file(&files, "get_user/parts.rs");
    assert!(parts.contents.contains("pub struct GetUserInput"));
    assert!(parts.contents.contains("impl GetUserInput"));
    assert!(
        parts
            .contents
            .contains("pub fn new(user_id: impl Into<String>) -> Self")
    );
    assert!(parts.contents.contains("pub fn include_details"));
    assert!(parts.contents.contains("pub enum GetUserResponse"));
    assert!(parts.contents.contains("pub fn get_user_parts"));
    assert!(!parts.contents.contains("#[cfg(feature = \"json\")]"));

    let json = find_file(&files, "get_user/json.rs");
    assert!(json.contents.contains("pub fn encode_get_user"));
    assert!(json.contents.contains("pub fn decode_get_user_response"));
    assert!(!json.contents.contains("#[cfg(feature = \"json\")]"));

    let parts = find_file(&files, "update_user/parts.rs");
    assert!(parts.contents.contains("pub struct UpdateUserInput"));
    assert!(parts.contents.contains("pub enum UpdateUserResponse"));
    assert!(parts.contents.contains("pub fn update_user_parts"));

    let json = find_file(&files, "update_user/json.rs");
    assert!(json.contents.contains("pub fn encode_update_user"));
    assert!(json.contents.contains("pub fn decode_update_user_response"));
}

#[test]
fn lib_root_module_option_emits_lib_rs_instead_of_mod_rs() {
    use satay_codegen::{GenerateOptions, RootModule};
    let options = GenerateOptions {
        root_module: RootModule::LibRs,
    };
    let files = satay_codegen::generate_with(SIMPLE, options).expect("generate simple fixture");

    let lib_rs = find_file(&files, "lib.rs");
    assert!(lib_rs.contents.contains("pub const SERVER_URL"));
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

    let api_rs = find_file(&files, "api.rs");
    assert!(
        api_rs
            .contents
            .contains("    /// Fetch a user.\n    pub fn get_user")
    );

    let types_rs = find_file(&files, "types.rs");
    assert!(types_rs.contents.contains("/// A user record.\n#[derive"));
    assert!(
        types_rs
            .contents
            .contains("    /// Stable ID.\n    pub id: String,")
    );
    assert!(types_rs.contents.contains("    /// Display name."));
    assert!(types_rs.contents.contains("    pub name: Option<String>,"));
    assert!(
        types_rs
            .contents
            .contains("    /// Reusable user code.\n    pub code: UserCode,")
    );
    assert!(
        types_rs
            .contents
            .contains("    /// Home user code.\n    pub home_code: UserCode,")
    );

    let parts_rs = find_file(&files, "get_user/parts.rs");
    assert!(parts_rs.contents.contains("/// Fetch a user.\n#[derive"));
    assert!(
        parts_rs
            .contents
            .contains("    /// User identifier.\n    pub user_id: String,")
    );
    assert!(
        parts_rs
            .contents
            .contains("    /// Include detailed fields.\n    pub include_details:")
    );
    assert!(
        parts_rs
            .contents
            .contains("    /// User found.\n    Ok(User)")
    );
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

    let mod_rs = find_file(&files, "mod.rs");
    assert!(
        mod_rs
            .contents
            .contains("pub const SERVER_URL: &str = \"https://api.example.test/v1\";")
    );

    let api_rs = find_file(&files, "api.rs");
    assert!(api_rs.contents.contains("pub fn account_key"));
    assert!(api_rs.contents.contains("pub fn api_key"));
    assert!(api_rs.contents.contains("pub fn get_user"));
    assert!(api_rs.contents.contains("pub struct GetUserAction"));
    assert!(api_rs.contents.contains("satay_runtime::insert_header"));
    assert!(api_rs.contents.contains("\"AccountKey\""));
    assert!(api_rs.contents.contains("satay_runtime::append_query_pair"));
    assert!(api_rs.contents.contains("\"api_key\""));
    assert!(api_rs.contents.contains("parts.uri = format!"));
    assert!(!api_rs.contents.contains("reqwest"));

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

    let mod_rs = find_file(&files, "get_user/mod.rs");
    assert!(mod_rs.contents.contains("#[cfg(feature = \"json\")]"));
    assert!(mod_rs.contents.contains("mod json;"));
    assert!(mod_rs.contents.contains("pub use json::*;"));

    let mod_rs = find_file(&files, "update_user/mod.rs");
    assert!(mod_rs.contents.contains("#[cfg(feature = \"json\")]"));
    assert!(mod_rs.contents.contains("mod json;"));
    assert!(mod_rs.contents.contains("pub use json::*;"));

    let parts = find_file(&files, "get_user/parts.rs");
    assert!(!parts.contents.contains("#[cfg(feature = \"json\")]"));

    let json = find_file(&files, "get_user/json.rs");
    assert!(!json.contents.contains("#[cfg(feature = \"json\")]"));
}

#[test]
fn petstore_minimal_fixture_generates_client_core() {
    let files = satay_codegen::generate(PETSTORE_MINIMAL).expect("generate petstore fixture");

    let types_rs = find_file(&files, "types.rs");
    assert!(types_rs.contents.contains("pub struct Pet"));

    let parts = find_file(&files, "list_pets/parts.rs");
    assert!(parts.contents.contains("pub fn list_pets_parts"));

    let json = find_file(&files, "create_pet/json.rs");
    assert!(json.contents.contains("pub fn encode_create_pet"));

    let json = find_file(&files, "get_pet/json.rs");
    assert!(json.contents.contains("pub fn decode_get_pet_response"));
}
