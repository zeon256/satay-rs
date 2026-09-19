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

    let parts = operations::get_user::get_user_parts(GetUserInput {
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

    let decoded: GetUserResponse =
        operations::get_user::decode_get_user_response(response.as_bytes())
            .expect("nullable nickname accepted");
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

    let err = operations::get_user::decode_get_user_response(response.as_bytes())
        .expect_err("invalid age rejected");
    assert!(err.to_string().contains("JSON error"));
}
