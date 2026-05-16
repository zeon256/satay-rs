use std::env;
use std::fs;
use std::path::Path;
use std::process;

use satay_codegen::{Error, GeneratedFile, ValidationError};

const SIMPLE: &str = include_str!("../../../tests/fixtures/simple.yaml");
const PETSTORE_MINIMAL: &str = include_str!("../../../tests/fixtures/petstore-minimal.yaml");
const CONSTRAINED: &str = include_str!("../../../tests/fixtures/constrained.yaml");
const INLINE_ENUM: &str = include_str!("../../../tests/fixtures/inline-enum.yaml");

fn find_file<'a>(files: &'a [GeneratedFile], relative_path: &str) -> &'a GeneratedFile {
    files
        .iter()
        .find(|f| f.relative_path == relative_path)
        .unwrap_or_else(|| {
            panic!(
                "expected file {relative_path}, found: {:?}",
                files.iter().map(|f| &f.relative_path).collect::<Vec<_>>()
            )
        })
}

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
fn server_security_and_api_action_helpers_are_generated() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.0.3
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

    let runtime_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("codegen crate has parent")
        .join("satay-runtime");
    let runtime_path = toml_string(&runtime_path.to_string_lossy());

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

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = process::Command::new(&cargo)
        .arg("test")
        .arg("--quiet")
        .current_dir(crate_dir)
        .output()
        .expect("run cargo test for secured generated crate");

    if !output.status.success() {
        panic!(
            "secured generated crate tests failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
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

#[test]
fn generated_simple_fixture_compiles_and_behaves() {
    let files = satay_codegen::generate(SIMPLE).expect("generate simple fixture");
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("codegen crate has parent")
        .join("satay-runtime");
    let runtime_path = toml_string(&runtime_path.to_string_lossy());

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn constructs_request_parts_without_io() {
        let parts = get_user_parts(GetUserInput::new("user/42").include_details(true))
        .expect("request parts");

        assert_eq!(parts.method, http::Method::GET);
        assert_eq!(parts.uri, "/users/user%2F42?includeDetails=true");
        assert_eq!(parts.headers.len(), 0);
        assert_eq!(parts.body, ());
    }

    #[test]
    fn action_builder_constructs_json_request_without_io() {
        let request = Api::new()
            .get_user("user/42")
            .include_details(true)
            .request()
            .expect("action request");

        assert_eq!(request.method(), http::Method::GET);
        assert_eq!(request.uri(), "/users/user%2F42?includeDetails=true");
        assert!(request.body().is_empty());
    }

    #[test]
    fn encodes_json_request_body() {
        let request = encode_update_user(
            UpdateUserInput::new("42")
            .notify(false)
            .body(UpdateUserRequest {
                age: None,
                name: "Ada".to_owned(),
            }),
        )
        .expect("encoded request");

        assert_eq!(request.method(), http::Method::PUT);
        assert_eq!(request.uri(), "/users/42?notify=false");
        assert_eq!(
            request.headers().get(http::header::CONTENT_TYPE).unwrap(),
            "application/vnd.satay.user+json"
        );

        let body: serde_json::Value = serde_json::from_slice(request.body()).unwrap();
        assert_eq!(body, serde_json::json!({ "name": "Ada" }));

        let empty_request = encode_update_user(UpdateUserInput::new("42"))
        .expect("encoded request without body");
        assert_eq!(empty_request.uri(), "/users/42");
        assert!(empty_request
            .headers()
            .get(http::header::CONTENT_TYPE)
            .is_none());
        assert!(empty_request.body().is_empty());
    }

    #[test]
    fn decodes_json_response_enums() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"42","name":"Ada","status":"active","age":36,"tags":["admin"]}"#.to_vec(),
        };
        let decoded = GetUserAction::decode(response).expect("decoded response");

        match decoded {
            GetUserResponse::Ok(user) => {
                assert_eq!(user.id, "42");
                assert_eq!(user.name, "Ada");
                assert_eq!(user.status, UserStatus::Active);
                assert_eq!(user.age, Some(36));
                assert_eq!(user.tags, Some(vec!["admin".to_owned()]));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn preserves_unexpected_response_body() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::INTERNAL_SERVER_ERROR,
            headers: http::HeaderMap::new(),
            body: b"server exploded".to_vec(),
        };
        let decoded = decode_get_user_response(response).expect("decoded response");

        match decoded {
            GetUserResponse::UnexpectedStatus(status, body) => {
                assert_eq!(status, http::StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body, b"server exploded");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = process::Command::new(&cargo)
        .arg("test")
        .arg("--quiet")
        .current_dir(crate_dir)
        .output()
        .expect("run cargo test for generated crate");

    if !output.status.success() {
        panic!(
            "generated crate tests failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn generated_constrained_fixture_enforces_openapi_bounds() {
    let files = satay_codegen::generate(CONSTRAINED).expect("generate constrained fixture");

    let types_rs = find_file(&files, "types.rs");
    assert!(types_rs.contents.contains("#[nutype::nutype("));
    assert!(
        types_rs
            .contents
            .contains("validate(greater_or_equal = 0, less_or_equal = 130)")
    );
    assert!(
        types_rs
            .contents
            .contains("validate(len_char_min = 1, len_char_max = 80)")
    );
    assert!(types_rs.contents.contains("regex = \"^[a-zA-Z0-9-]+$\""));

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("codegen crate has parent")
        .join("satay-runtime");
    let runtime_path = toml_string(&runtime_path.to_string_lossy());

    write_manifest(crate_dir, &runtime_path, true, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn rejects_invalid_values_at_construction() {
        assert!(Age::try_new(-1).is_err());
        assert!(Age::try_new(131).is_err());
        assert!(UserName::try_new(String::new()).is_err());
        assert!(UserName::try_new("a".repeat(81)).is_err());
        assert!(GetUserTagsParameter::try_new(Vec::new()).is_err());
    }

    #[test]
    fn regex_validation_rejects_invalid_patterns() {
        assert!(Email::try_new("not-an-email".to_owned()).is_err());
        assert!(Email::try_new("user@domain.com".to_owned()).is_ok());
        assert!(Slug::try_new("hello world".to_owned()).is_err());
        assert!(Slug::try_new("hello-world".to_owned()).is_ok());
    }

    #[test]
    fn request_parts_use_validated_values() {
        let user_id = GetUserUserIdParameter::try_new("user-42".to_owned()).unwrap();
        let limit = GetUserLimitParameter::try_new(10).unwrap();
        let tag = GetUserTagsParameterItem::try_new("rs".to_owned()).unwrap();
        let tags = GetUserTagsParameter::try_new(vec![tag]).unwrap();

        let parts = get_user_parts(GetUserInput {
            user_id,
            limit: Some(limit),
            tags: Some(tags),
        })
        .expect("request parts");

        assert_eq!(parts.uri, "/users/user-42?limit=10&tags=rs");
    }

    #[test]
    fn response_deserialization_enforces_bounds() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"42","name":"Ada","age":131,"score":0.5}"#.to_vec(),
        };

        let err = decode_get_user_response(response).expect_err("invalid age rejected");
        assert!(err.to_string().contains("JSON error"));
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = process::Command::new(&cargo)
        .arg("test")
        .arg("--quiet")
        .current_dir(crate_dir)
        .output()
        .expect("run cargo test for constrained generated crate");

    if !output.status.success() {
        panic!(
            "constrained generated crate tests failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = process::Command::new(cargo)
        .arg("check")
        .arg("--quiet")
        .arg("--no-default-features")
        .current_dir(crate_dir)
        .output()
        .expect("run cargo check for constrained generated crate without features");

    if !output.status.success() {
        panic!(
            "constrained generated crate no-default check failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn x_satay_parse_as_generates_string_backed_deserializers() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.0.3
paths:
  /readings:
    get:
      operationId: getReading
      responses:
        '200':
          description: Reading
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Reading'
components:
  schemas:
    Reading:
      type: object
      required:
        - id
        - value
        - count
        - seenAt
      properties:
        id:
          type: string
          x-satay:
            parse-as: u32
        value:
          type: string
          x-satay:
            parse-as: f64
        count:
          type: string
          x-satay:
            parse-as: u8
        seenAt:
          type: string
          x-satay:
            parse-as: offset-datetime
"#,
    )
    .expect("generate parse-as fixture");

    let types_rs = find_file(&files, "types.rs");
    assert!(types_rs.contents.contains("pub id: u32"));
    assert!(types_rs.contents.contains("pub value: f64"));
    assert!(types_rs.contents.contains("pub count: u8"));
    assert!(
        types_rs
            .contents
            .contains("pub seen_at: satay_runtime::OffsetDateTime")
    );
    assert!(
        types_rs
            .contents
            .contains("with = \"satay_runtime::serde_string::as_u32\"")
    );
    assert!(
        types_rs
            .contents
            .contains("with = \"satay_runtime::serde_string::as_f64\"")
    );
    assert!(
        types_rs
            .contents
            .contains("with = \"satay_runtime::serde_string::as_offset_datetime\"")
    );

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("codegen crate has parent")
        .join("satay-runtime");
    let runtime_path = toml_string(&runtime_path.to_string_lossy());

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn decodes_and_encodes_string_backed_values() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"42","value":"1.25","count":"7","seenAt":"2024-08-14T16:41:48+08:00"}"#
                .to_vec(),
        };
        let decoded = decode_get_reading_response(response).expect("decoded response");

        match decoded {
            GetReadingResponse::Ok(reading) => {
                assert_eq!(reading.id, 42);
                assert_eq!(reading.value, 1.25);
                assert_eq!(reading.count, 7);
                assert_eq!(reading.seen_at.offset().whole_hours(), 8);

                let encoded = serde_json::to_value(&reading).unwrap();
                assert_eq!(
                    encoded,
                    serde_json::json!({
                        "id": "42",
                        "value": "1.25",
                        "count": "7",
                        "seenAt": "2024-08-14T16:41:48+08:00"
                    })
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = process::Command::new(&cargo)
        .arg("test")
        .arg("--quiet")
        .current_dir(crate_dir)
        .output()
        .expect("run cargo test for parse-as generated crate");

    if !output.status.success() {
        panic!(
            "parse-as generated crate tests failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn nullable_parameters_are_rejected_instead_of_generating_invalid_rust() {
    let err = satay_codegen::generate(
        r#"
openapi: 3.0.3
info:
  title: Nullable parameter
  version: 1.0.0
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
            nullable: true
      responses:
        '204':
          description: No content
"#,
    )
    .expect_err("nullable parameters are unsupported");

    match err {
        Error::Validation(ValidationError::NullableParameterUnsupported { wire_name, .. }) => {
            assert_eq!(wire_name, "userId");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn default_response_bodies_are_rejected_instead_of_silently_dropped() {
    let err = satay_codegen::generate(
        r#"
openapi: 3.0.3
info:
  title: Default response
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        default:
          description: Error response
          content:
            application/json:
              schema:
                type: string
"#,
    )
    .expect_err("default response bodies are unsupported");

    match err {
        Error::Validation(ValidationError::DefaultResponseBodyUnsupported { context, .. }) => {
            assert_eq!(context, "operation `ping` responses");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn inline_enum_generates_proper_enum_types() {
    let files = satay_codegen::generate(INLINE_ENUM).expect("generate inline-enum fixture");

    let types_rs = find_file(&files, "types.rs");
    assert!(types_rs.contents.contains("pub enum ItemCategory"));
    assert!(types_rs.contents.contains("pub enum ItemCondition"));
    assert!(types_rs.contents.contains("pub struct Item"));
    assert!(types_rs.contents.contains("#[default]"));
    assert!(types_rs.contents.contains("serde(other)"));
    assert!(!types_rs.contents.contains(r#"rename = """#));

    let item_struct_start = types_rs
        .contents
        .find("pub struct Item")
        .expect("Item struct exists");
    let item_struct = &types_rs.contents[item_struct_start..];
    assert!(item_struct.contains("category: ItemCategory"));
    assert!(item_struct.contains("condition: ItemCondition"));

    let category_enum_start = types_rs
        .contents
        .find("pub enum ItemCategory")
        .expect("ItemCategory enum exists");
    let category_enum = &types_rs.contents[category_enum_start..category_enum_start + 400];
    assert!(category_enum.contains("Electronics"));
    assert!(category_enum.contains("Clothing"));
    assert!(category_enum.contains("Food"));
    assert!(category_enum.contains("Unknown"));

    let condition_enum_start = types_rs
        .contents
        .find("pub enum ItemCondition")
        .expect("ItemCondition enum exists");
    let condition_enum = &types_rs.contents[condition_enum_start..condition_enum_start + 400];
    assert!(condition_enum.contains("New"));
    assert!(condition_enum.contains("Used"));
    assert!(condition_enum.contains("Refurbished"));
    assert!(condition_enum.contains("Unknown"));
}

#[test]
fn x_satay_enum_variants_generate_named_variants() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.0.3
info:
  title: Enum Variants API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BusArrivalTiming'
components:
  schemas:
    BusArrivalTiming:
      type: object
      required:
        - Type
      properties:
        Type:
          type: string
          description: Vehicle type.
          enum:
            - SD
            - DD
            - BD
            - ""
          example: SD
          x-satay:
            enum-variants:
              SD: SingleDecker
              DD: DoubleDecker
              BD: Bendy
              "": Unknown
"#,
    )
    .expect("generate enum variants fixture");

    let types_rs = find_file(&files, "types.rs");
    let enum_start = types_rs
        .contents
        .find("pub enum BusArrivalTimingType")
        .expect("BusArrivalTimingType enum exists");
    let enum_contents = &types_rs.contents[enum_start..enum_start + 600];

    assert!(enum_contents.contains("SingleDecker"));
    assert!(enum_contents.contains("DoubleDecker"));
    assert!(enum_contents.contains("Bendy"));
    assert!(enum_contents.contains(r#"serde(rename = "SD")"#));
    assert!(enum_contents.contains(r#"serde(rename = "DD")"#));
    assert!(enum_contents.contains(r#"serde(rename = "BD")"#));
    assert!(enum_contents.contains("serde(other)"));
    assert!(!enum_contents.contains("Sd"));
    assert!(!enum_contents.contains("Dd"));
    assert!(!enum_contents.contains("Bd"));
    assert!(!enum_contents.contains(r#"rename = """#));
}

#[test]
fn generated_inline_enum_compiles_and_handles_unknown() {
    let files = satay_codegen::generate(INLINE_ENUM).expect("generate inline-enum fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("codegen crate has parent")
        .join("satay-runtime");
    let runtime_path = toml_string(&runtime_path.to_string_lossy());

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn known_enum_variants_deserialize() {
        let json = br#"{"id":"1","name":"Widget","category":"electronics","condition":"new","notes":"test"}"#.to_vec();
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: json,
        };
        let decoded = decode_get_item_response(response).expect("decoded response");
        match decoded {
            GetItemResponse::Ok(item) => {
                assert_eq!(item.id, "1");
                assert_eq!(item.name, "Widget");
                assert_eq!(item.category, ItemCategory::Electronics);
                assert_eq!(item.condition, ItemCondition::New);
                assert_eq!(item.notes, Some("test".to_owned()));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn unknown_enum_variant_maps_to_unknown() {
        let json = br#"{"id":"2","name":"Gadget","category":"unknown_category","condition":"","notes":null}"#.to_vec();
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: json,
        };
        let decoded = decode_get_item_response(response).expect("decoded response");
        match decoded {
            GetItemResponse::Ok(item) => {
                assert_eq!(item.category, ItemCategory::Unknown);
                assert_eq!(item.condition, ItemCondition::Unknown);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn default_variant_is_unknown() {
        assert_eq!(ItemCategory::default(), ItemCategory::Unknown);
        assert_eq!(ItemCondition::default(), ItemCondition::Unknown);
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = process::Command::new(&cargo)
        .arg("test")
        .arg("--quiet")
        .current_dir(crate_dir)
        .output()
        .expect("run cargo test for inline-enum generated crate");

    if !output.status.success() {
        panic!(
            "inline-enum generated crate tests failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn write_manifest(
    crate_dir: &Path,
    runtime_path: &str,
    constrained: bool,
    _for_compile_test: bool,
) {
    let nutype_deps = if constrained {
        r#"
nutype = { version = "0.7", features = ["serde", "regex"] }
regex = "1"
"#
    } else {
        ""
    };

    let manifest = format!(
        r#"[package]
name = "satay-generated-check"
version = "0.0.0"
edition = "2024"

[features]
default = ["serde", "json"]
serde = ["dep:serde", "satay-runtime/serde"]
json = ["serde", "dep:serde_json", "satay-runtime/json"]

[dependencies]
http = "1"
satay-runtime = {{ path = {runtime_path}, default-features = false }}
serde = {{ version = "1", features = ["derive"], optional = true }}
serde_json = {{ version = "1", optional = true }}
{nutype_deps}"#
    );
    fs::create_dir_all(crate_dir.join("src")).expect("create src dir");
    fs::write(crate_dir.join("Cargo.toml"), manifest).expect("write manifest");
}

fn write_generated_files(generated_dir: &Path, files: &[GeneratedFile]) {
    for file in files {
        let path = generated_dir.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create generated subdir");
        }
        fs::write(&path, &file.contents).expect("write generated file");
    }
}

fn toml_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
