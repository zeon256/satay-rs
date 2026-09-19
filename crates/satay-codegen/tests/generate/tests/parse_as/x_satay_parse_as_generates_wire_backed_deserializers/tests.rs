use super::generated::*;

#[test]
fn decodes_and_encodes_string_backed_values() {
    let parts = operations::get_reading::get_reading_parts(GetReadingInput::new(42))
        .expect("request parts");
    assert_eq!(parts.uri, "/readings?readingId=42");

    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"id":"42","value":"1.25","count":"7","monitored":0,"seenAt":"2024-08-14T16:41:48+08:00","startsAt":"0620","noServiceAt":"","aliasId":"42","frequency":"14-17","tolerance":"1.5-2.75"}"#
            .to_vec(),
    };
    let decoded: GetReadingResponse =
        operations::get_reading::decode_get_reading_response(response.as_bytes())
            .expect("decoded response");

    match decoded {
        GetReadingResponse::Ok(reading) => {
            assert_eq!(reading.id, 42);
            assert_eq!(reading.value, 1.25);
            assert_eq!(reading.count, 7);
            assert!(!reading.monitored);
            assert_eq!(reading.seen_at.offset().whole_hours(), 8);
            let starts_at = reading.starts_at.expect("startsAt parsed");
            assert_eq!(starts_at.hour(), 6);
            assert_eq!(starts_at.minute(), 20);
            assert_eq!(reading.no_service_at, None);
            assert_eq!(reading.alias_id, 42);
            assert_eq!(reading.frequency.min, Some(14));
            assert_eq!(reading.frequency.max, Some(17));
            assert_eq!(reading.tolerance.min, Some(1.5));
            assert_eq!(reading.tolerance.max, Some(2.75));

            let encoded = serde_json::to_value(&reading).unwrap();
            assert_eq!(
                encoded,
                serde_json::json!({
                    "id": "42",
                    "value": "1.25",
                    "count": "7",
                    "monitored": 0,
                    "seenAt": "2024-08-14T16:41:48+08:00",
                    "startsAt": "0620",
                    "noServiceAt": null,
                    "aliasId": "42",
                    "frequency": "14-17",
                    "tolerance": "1.5-2.75"
                })
            );
        }
        other => panic!("unexpected response: {other:?}"),
    }
}
