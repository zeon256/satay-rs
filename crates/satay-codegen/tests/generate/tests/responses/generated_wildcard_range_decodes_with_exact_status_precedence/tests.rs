use super::generated::*;

#[test]
fn decodes_range_body_with_concrete_status() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::TOO_MANY_REQUESTS,
        headers: http::HeaderMap::new(),
        body: br#"{"message":"slow down"}"#.to_vec(),
    };
    let decoded: GetUserResponse =
        operations::get_user::decode_get_user_response(response.as_bytes())
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
    let decoded: GetUserResponse =
        operations::get_user::decode_get_user_response(response.as_bytes())
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
    let decoded: GetUserResponse =
        operations::get_user::decode_get_user_response(response.as_bytes())
            .expect("decoded response");

    match decoded {
        GetUserResponse::UnexpectedStatus(status, body) => {
            assert_eq!(status, http::StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(body, b"boom");
        }
        other => panic!("unexpected response: {other:?}"),
    }
}
