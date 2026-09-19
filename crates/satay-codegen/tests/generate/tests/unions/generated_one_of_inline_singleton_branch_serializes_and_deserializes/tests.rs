use super::generated::*;

#[test]
fn one_of_inline_singleton_deserializes_string_branch() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#""auto""#.to_vec(),
    };

    let decoded: GetFormatResponse =
        operations::get_format::decode_get_format_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetFormatResponse::Ok(AssistantsApiResponseFormatOption::Auto(value)) => {
            assert_eq!(value, AssistantsApiResponseFormatOptionAuto::Auto);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn one_of_inline_singleton_deserializes_object_branch() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"type":"json_object"}"#.to_vec(),
    };

    let decoded: GetFormatResponse =
        operations::get_format::decode_get_format_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetFormatResponse::Ok(AssistantsApiResponseFormatOption::ResponseFormatJsonObject(
            value,
        )) => {
            assert_eq!(value.r#type, ResponseFormatJsonObjectType::JsonObject);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn one_of_inline_singleton_serializes_string_branch() {
    let value =
        AssistantsApiResponseFormatOption::Auto(AssistantsApiResponseFormatOptionAuto::Auto);
    let encoded = serde_json::to_value(value).expect("serialized response format");
    assert_eq!(encoded, serde_json::json!("auto"));
}
