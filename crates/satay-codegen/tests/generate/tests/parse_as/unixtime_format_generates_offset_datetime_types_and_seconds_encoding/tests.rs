use super::generated::*;

#[test]
fn encodes_unixtime_query_parameter_and_json_values() {
    let at = satay_runtime::OffsetDateTime::from_unix_timestamp(1_719_892_800).unwrap();
    let before_epoch = satay_runtime::OffsetDateTime::from_unix_timestamp(-1).unwrap();

    let parts =
        operations::get_events::get_events_parts(GetEventsInput::new(at)).expect("request parts");
    assert_eq!(parts.uri, "/events?at=1719892800");

    let event: Event = serde_json::from_value(serde_json::json!({
        "startedAt": 1719892800,
        "endedAt": null,
        "createdAtString": "1719892800",
        "endedAtString": "-1"
    }))
    .unwrap();

    assert_eq!(event.started_at, at);
    assert_eq!(event.ended_at, None);
    assert_eq!(event.created_at_string, at);
    assert_eq!(event.ended_at_string, Some(before_epoch));

    let encoded = serde_json::to_value(event).unwrap();
    assert_eq!(
        encoded,
        serde_json::json!({
            "startedAt": 1719892800,
            "endedAt": null,
            "createdAtString": "1719892800",
            "endedAtString": "-1"
        })
    );
}
