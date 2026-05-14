use std::fs;
use std::path::Path;
use std::process::Command;

const SIMPLE: &str = include_str!("../../../tests/fixtures/simple.yaml");
const SIMPLE_EXPECTED: &str = include_str!("../../../tests/fixtures/simple.expected.rs");
const PETSTORE_MINIMAL: &str = include_str!("../../../tests/fixtures/petstore-minimal.yaml");
const CONSTRAINED: &str = include_str!("../../../tests/fixtures/constrained.yaml");

#[test]
fn simple_fixture_matches_golden_file() {
    let generated = satay_codegen::generate(SIMPLE).expect("generate simple fixture");
    assert_eq!(generated, SIMPLE_EXPECTED);
}

#[test]
fn petstore_minimal_fixture_generates_client_core() {
    let generated = satay_codegen::generate(PETSTORE_MINIMAL).expect("generate petstore fixture");

    assert!(generated.contains("pub struct Pet"));
    assert!(generated.contains("pub fn list_pets_parts"));
    assert!(generated.contains("pub fn encode_create_pet"));
    assert!(generated.contains("pub fn decode_get_pet_response"));
}

#[test]
fn generated_simple_fixture_compiles_and_behaves() {
    let generated = satay_codegen::generate(SIMPLE).expect("generate simple fixture");
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    fs::create_dir(crate_dir.join("src")).expect("create src dir");

    let runtime_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("codegen crate has parent")
        .join("satay-runtime");
    let runtime_path = toml_string(&runtime_path.to_string_lossy());

    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
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
"#
        ),
    )
    .expect("write manifest");

    fs::write(crate_dir.join("src/generated.rs"), generated).expect("write generated module");
    fs::write(
        crate_dir.join("src/lib.rs"),
        r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn constructs_request_parts_without_io() {
        let parts = get_user_parts(GetUserInput {
            user_id: "user/42".to_owned(),
            include_details: Some(true),
        })
        .expect("request parts");

        assert_eq!(parts.method, http::Method::GET);
        assert_eq!(parts.uri, "/users/user%2F42?includeDetails=true");
        assert_eq!(parts.headers.len(), 0);
        assert_eq!(parts.body, ());
    }

    #[test]
    fn encodes_json_request_body() {
        let request = encode_update_user(UpdateUserInput {
            user_id: "42".to_owned(),
            notify: Some(false),
            body: Some(UpdateUserRequest {
                age: None,
                name: "Ada".to_owned(),
            }),
        })
        .expect("encoded request");

        assert_eq!(request.method(), http::Method::PUT);
        assert_eq!(request.uri(), "/users/42?notify=false");
        assert_eq!(
            request.headers().get(http::header::CONTENT_TYPE).unwrap(),
            "application/vnd.satay.user+json"
        );

        let body: serde_json::Value = serde_json::from_slice(request.body()).unwrap();
        assert_eq!(body, serde_json::json!({ "name": "Ada" }));

        let empty_request = encode_update_user(UpdateUserInput {
            user_id: "42".to_owned(),
            notify: None,
            body: None,
        })
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
        let response = http::Response::builder()
            .status(200)
            .body(
                br#"{"id":"42","name":"Ada","status":"active","age":36,"tags":["admin"]}"#
                    .to_vec(),
            )
            .unwrap();
        let decoded = decode_get_user_response(response).expect("decoded response");

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
        let response = http::Response::builder()
            .status(500)
            .body(b"server exploded".to_vec())
            .unwrap();
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
"##,
    )
    .expect("write lib");

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
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
    let generated = satay_codegen::generate(CONSTRAINED).expect("generate constrained fixture");
    assert!(generated.contains("#[nutype::nutype("));
    assert!(generated.contains("validate(greater_or_equal = 0, less_or_equal = 130)"));
    assert!(generated.contains("validate(len_char_min = 1, len_char_max = 80)"));

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    fs::create_dir(crate_dir.join("src")).expect("create src dir");

    let runtime_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("codegen crate has parent")
        .join("satay-runtime");
    let runtime_path = toml_string(&runtime_path.to_string_lossy());

    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "satay-generated-constrained-check"
version = "0.0.0"
edition = "2024"

[features]
default = ["serde", "json"]
serde = ["dep:serde", "satay-runtime/serde"]
json = ["serde", "dep:serde_json", "satay-runtime/json"]

[dependencies]
http = "1"
nutype = {{ version = "0.7", features = ["serde"] }}
satay-runtime = {{ path = {runtime_path}, default-features = false }}
serde = {{ version = "1", features = ["derive"], optional = true }}
serde_json = {{ version = "1", optional = true }}
"#
        ),
    )
    .expect("write manifest");

    fs::write(crate_dir.join("src/generated.rs"), generated).expect("write generated module");
    fs::write(
        crate_dir.join("src/lib.rs"),
        r##"pub mod generated;

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
        let response = http::Response::builder()
            .status(200)
            .body(br#"{"id":"42","name":"Ada","age":131,"score":0.5}"#.to_vec())
            .unwrap();

        let err = decode_get_user_response(response).expect_err("invalid age rejected");
        assert!(err.to_string().contains("JSON error"));
    }
}
"##,
    )
    .expect("write lib");

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(&cargo)
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

    let output = Command::new(cargo)
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
        satay_codegen::Error::Validation(
            satay_codegen::ValidationError::NullableParameterUnsupported { wire_name, .. },
        ) => {
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
        satay_codegen::Error::Validation(
            satay_codegen::ValidationError::DefaultResponseBodyUnsupported { context, .. },
        ) => {
            assert_eq!(context, "operation `ping` responses");
        }
        other => panic!("unexpected error: {other}"),
    }
}

fn toml_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
