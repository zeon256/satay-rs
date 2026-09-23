use super::generated::*;

#[test]
fn constructs_request_parts_without_io() {
    let parts = operations::get_user::get_user_parts(
        GetUserInput::<satay_runtime::storage::AllocStorage>::new("user/42").include_details(true),
    )
    .expect("request parts");

    assert_eq!(parts.method, http::Method::GET);
    assert_eq!(parts.uri, "/users/user%2F42?includeDetails=true");
    assert_eq!(parts.headers.len(), 0);
    assert_eq!(parts.body, ());
}

#[test]
fn action_builder_constructs_json_request_without_io() {
    let request = Api::new()
        .users()
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
    let request = operations::update_user::encode_update_user(
        UpdateUserInput::<satay_runtime::storage::AllocStorage>::new("42")
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

    let empty_request =
        operations::update_user::encode_update_user(UpdateUserInput::<satay_runtime::storage::AllocStorage>::new("42"))
            .expect("encoded request without body");
    assert_eq!(empty_request.uri(), "/users/42");
    assert!(
        empty_request
            .headers()
            .get(http::header::CONTENT_TYPE)
            .is_none()
    );
    assert!(empty_request.body().is_empty());
}

#[test]
fn decodes_json_response_enums() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"id":"42","name":"Ada","status":"active","age":36,"tags":["admin"]}"#.to_vec(),
    };
    let decoded = operations::get_user::GetUserAction::<satay_runtime::storage::AllocStorage>::decode(response.as_bytes())
        .expect("decoded response");

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
    let decoded: GetUserResponse =
        operations::get_user::decode_get_user_response(response.as_bytes())
            .expect("decoded response");

    match decoded {
        GetUserResponse::UnexpectedStatus(status, body) => {
            assert_eq!(status, http::StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(body, b"server exploded");
        }
        other => panic!("unexpected response: {other:?}"),
    }
}
