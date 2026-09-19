use super::generated::*;

#[test]
fn decodes_inline_all_of_array_items() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"object":"list","data":[{"role":"user","content":"hello","id":"chatcmpl-1-0","content_parts":[{"type":"text","text":"hello"}]}],"first_id":"chatcmpl-1-0","last_id":"chatcmpl-1-0","has_more":false}"#.to_vec(),
    };

    let decoded: ListMessagesResponse =
        operations::list_messages::decode_list_messages_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        ListMessagesResponse::Ok(list) => {
            assert_eq!(list.data.len(), 1);
            let item = &list.data[0];
            assert_eq!(item.role, "user");
            assert_eq!(item.content, "hello");
            assert_eq!(item.id, "chatcmpl-1-0");
            assert_eq!(item.content_parts.as_ref().expect("content parts").len(), 1);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}
