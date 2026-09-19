use super::generated::*;

#[test]
fn string_content_deserializes_to_string_variant() {
    let value: Message =
        serde_json::from_str(r#"{"content":"hello"}"#).expect("message with string content");

    match value.content {
        Some(MessageContent::String(text)) => assert_eq!(text, "hello"),
        other => panic!("unexpected content: {other:?}"),
    }
}

#[test]
fn array_content_deserializes_to_array_variant() {
    let value: Message = serde_json::from_str(r#"{"content":[{"type":"text","text":"hello"}]}"#)
        .expect("message with array content");

    match value.content {
        Some(MessageContent::Array(parts)) => {
            assert_eq!(parts.len(), 1);
            assert_eq!(parts[0].text, "hello");
        }
        other => panic!("unexpected content: {other:?}"),
    }
}

#[test]
fn null_content_deserializes_to_none() {
    let value: Message =
        serde_json::from_str(r#"{"content":null}"#).expect("message with null content");

    assert_eq!(value.content, None);
}

#[test]
fn absent_optional_content_serializes_as_absent() {
    let value: Message = Message { content: None };
    let encoded = serde_json::to_value(value).expect("serialized message");

    assert_eq!(encoded, serde_json::json!({}));
}
