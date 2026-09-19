use super::generated::*;

#[test]
fn decodes_flattened_all_of_fields() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"id":"base-1","name":"Ada","nickname":"ace"}"#.to_vec(),
    };

    let decoded: GetChildResponse =
        operations::get_child::decode_get_child_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetChildResponse::Ok(child) => {
            assert_eq!(child.id, "base-1");
            assert_eq!(child.name, "Ada");
            assert_eq!(child.nickname, Some("ace".to_owned()));
        }
        other => panic!("unexpected response: {other:?}"),
    }
}
