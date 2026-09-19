use super::generated::*;

#[test]
fn one_of_inline_multi_value_deserializes_string_branch() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#""auto""#.to_vec(),
    };

    let decoded: GetToolChoiceResponse =
        operations::get_tool_choice::decode_get_tool_choice_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetToolChoiceResponse::Ok(AssistantsApiToolChoiceOption::Enum(value)) => {
            assert_eq!(value, AssistantsApiToolChoiceOptionEnum::Auto);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn one_of_inline_multi_value_deserializes_object_branch() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"type":"function","function":{"name":"my_function"}}"#.to_vec(),
    };

    let decoded: GetToolChoiceResponse =
        operations::get_tool_choice::decode_get_tool_choice_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetToolChoiceResponse::Ok(AssistantsApiToolChoiceOption::AssistantsNamedToolChoice(
            value,
        )) => {
            assert_eq!(value.r#type, AssistantsNamedToolChoiceType::Function);
            assert_eq!(value.function.expect("function choice").name, "my_function");
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn one_of_inline_multi_value_serializes_string_branch() {
    let value: AssistantsApiToolChoiceOption =
        AssistantsApiToolChoiceOption::Enum(AssistantsApiToolChoiceOptionEnum::Required);
    let encoded = serde_json::to_value(value).expect("serialized tool choice");
    assert_eq!(encoded, serde_json::json!("required"));
}
