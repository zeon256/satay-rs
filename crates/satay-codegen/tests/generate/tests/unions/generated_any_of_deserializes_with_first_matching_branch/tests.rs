use super::generated::*;

#[test]
fn any_of_uses_first_matching_branch() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"id":"1","slug":"specific"}"#.to_vec(),
    };

    let decoded: GetEntityResponse =
        operations::get_entity::decode_get_entity_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetEntityResponse::Ok(Entity::Loose(value)) => {
            assert_eq!(value.id, "1");
        }
        other => panic!("unexpected response: {other:?}"),
    }
}
