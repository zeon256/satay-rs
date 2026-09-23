use super::generated::*;

#[test]
fn nested_union_deserializes_by_embedded_tag() {
    let widget: Widget =
        serde_json::from_str(r#"{"id":"w1","status":{"type":"on","since":"today"}}"#)
            .expect("deserialized widget");
    match widget.status {
        Some(WidgetStatus::StatusOn(status)) => {
            assert_eq!(status.r#type, StatusOnType::On);
            assert_eq!(status.since, "today");
        }
        other => panic!("unexpected status: {other:?}"),
    }
}

#[test]
fn nested_union_serializes_embedded_tag() {
    let widget: Widget = Widget {
        id: "w2".to_owned(),
        status: Some(WidgetStatus::StatusOff(StatusOff {
            r#type: StatusOffType::Off,
        })),
    };
    let encoded = serde_json::to_value(widget).expect("serialized widget");
    assert_eq!(
        encoded,
        serde_json::json!({
            "id": "w2",
            "status": {"type": "off"}
        })
    );
}

#[test]
fn nested_union_null_round_trips() {
    let widget: Widget = serde_json::from_str(r#"{"id":"w3","status":null}"#)
        .expect("deserialized widget with null status");
    assert!(widget.status.is_none());
}
