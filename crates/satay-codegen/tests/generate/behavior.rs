use std::fs;

use crate::ast::*;
use crate::common::*;

#[test]
fn generated_simple_fixture_compiles_and_behaves() {
    let files = satay_codegen::generate(SIMPLE).expect("generate simple fixture");
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

    run_temp_cargo(crate_dir, "test", &[], "generated crate tests");
}

#[test]
fn generated_response_name_collision_compiles_and_decodes() {
    let files =
        satay_codegen::generate(RESPONSE_NAME_COLLISION).expect("generate collision fixture");

    let parts = parse_rust(find_file(&files, "psi/parts.rs"));
    let response = find_enum(&parts, "PsiOperationResponse");
    assert_eq!(
        norm(&variant(response, "Ok").fields),
        norm_str("(PsiResponse)")
    );
    assert!(!has_enum(&parts, "PsiResponse"));

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
    fn decodes_nonrecursive_collision_response() {
        let expected = PsiResponse { value: 42 };
        let manual = PsiOperationResponse::Ok(expected.clone());
        assert_eq!(manual, PsiOperationResponse::Ok(expected));

        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"value":42}"#.to_vec(),
        };
        let decoded = PsiAction::decode(response).expect("decoded response");

        assert_eq!(decoded, PsiOperationResponse::Ok(PsiResponse { value: 42 }));
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "response collision generated crate tests",
    );
}

#[test]
fn generated_constrained_fixture_enforces_openapi_bounds() {
    let files = satay_codegen::generate(CONSTRAINED).expect("generate constrained fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    assert_tuple_struct(&types_rs, "Age", "u8");
    assert_attr_contains(
        &find_struct(&types_rs, "Age").attrs,
        "nutype::nutype",
        "validate(less_or_equal = 130)",
    );
    assert_attr_contains(
        &find_struct(&types_rs, "UserName").attrs,
        "nutype::nutype",
        "validate(len_char_min = 1, len_char_max = 80)",
    );
    assert_attr_contains(
        &find_struct(&types_rs, "UserNickname").attrs,
        "nutype::nutype",
        "validate(len_char_min = 1, len_char_max = 40)",
    );
    assert_attr_contains(
        &find_struct(&types_rs, "UserScore").attrs,
        "nutype::nutype",
        "validate(finite, greater = 0.0, less = 1.0)",
    );
    assert_field(
        find_struct(&types_rs, "User"),
        "nickname",
        "Option<UserNickname>",
    );
    assert_attr_contains(
        &find_struct(&types_rs, "GetUserUserIdParameter").attrs,
        "nutype::nutype",
        r#"regex = "^[a-zA-Z0-9-]+$""#,
    );

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, true, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn rejects_invalid_values_at_construction() {
        assert!(Age::try_new(131).is_err());
        assert!(GetUserLimitParameter::try_new(0).is_err());
        assert!(UserName::try_new(String::new()).is_err());
        assert!(UserName::try_new("a".repeat(81)).is_err());
        assert!(UserNickname::try_new(String::new()).is_err());
        assert!(UserNickname::try_new("a".repeat(41)).is_err());
        assert!(UserScore::try_new(0.0).is_err());
        assert!(UserScore::try_new(1.0).is_err());
        assert!(UserScore::try_new(0.5).is_ok());
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
    fn response_deserialization_accepts_31_nullable_type_arrays() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"42","name":"Ada","nickname":null,"age":36,"score":0.5}"#.to_vec(),
        };

        let decoded = decode_get_user_response(response).expect("nullable nickname accepted");
        match decoded {
            GetUserResponse::Ok(user) => assert!(user.nickname.is_none()),
            other => panic!("unexpected response: {other:?}"),
        }
    }
    #[test]
    fn response_deserialization_enforces_bounds() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"42","name":"Ada","nickname":null,"age":131,"score":0.5}"#.to_vec(),
        };

        let err = decode_get_user_response(response).expect_err("invalid age rejected");
        assert!(err.to_string().contains("JSON error"));
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "constrained generated crate tests");
    run_temp_cargo(
        crate_dir,
        "check",
        &["--no-default-features"],
        "constrained generated crate no-default check",
    );
}
