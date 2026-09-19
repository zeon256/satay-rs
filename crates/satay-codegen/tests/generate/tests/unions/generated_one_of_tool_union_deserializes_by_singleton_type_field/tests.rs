use super::generated::*;

#[test]
fn one_of_tool_union_deserializes_by_type_field() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"tools":[{"type":"function","function":"lookup"}]}"#.to_vec(),
    };

    let decoded: GetAssistantResponse =
        operations::get_assistant::decode_get_assistant_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetAssistantResponse::Ok(value) => match &value.tools[0] {
            AssistantObjectToolsItem::AssistantToolsFunction(tool) => {
                assert_eq!(tool.function, "lookup");
            }
            other => panic!("unexpected tool: {other:?}"),
        },
        other => panic!("unexpected response: {other:?}"),
    }
}
