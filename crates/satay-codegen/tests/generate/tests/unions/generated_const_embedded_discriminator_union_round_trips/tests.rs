use super::generated::*;

#[test]
fn const_tag_union_deserializes_response() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"id":"call_1","type":"custom","custom":"payload"}"#.to_vec(),
    };

    let decoded: GetToolResponse =
        operations::get_tool::decode_get_tool_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetToolResponse::Ok(ToolCall::CustomToolCall(value)) => {
            assert_eq!(value.id, "call_1");
            assert_eq!(value.r#type, CustomToolCallType::Custom);
            assert_eq!(value.custom, "payload");
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn const_tag_union_serializes_branch_type() {
    let value = ToolCall::FunctionToolCall(FunctionToolCall {
        id: "call_2".to_owned(),
        r#type: FunctionToolCallType::Function,
        function: "lookup".to_owned(),
    });
    let encoded = serde_json::to_value(value).expect("serialized tool call");
    assert_eq!(
        encoded,
        serde_json::json!({
            "id": "call_2",
            "type": "function",
            "function": "lookup"
        })
    );
}
