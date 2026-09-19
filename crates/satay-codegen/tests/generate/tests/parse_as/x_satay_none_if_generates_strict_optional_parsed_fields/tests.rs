use super::generated::Reading;

#[test]
fn sentinel_fields_decode_and_encode_strictly() {
    let valid: Reading = serde_json::from_str(
        r#"{"requiredWbgt":"28.7","optionalWbgt":"17.5","nullableWbgt":"12.0","maximumSpeed":"88"}"#,
    )
    .unwrap();
    assert_eq!(valid.required_wbgt, Some(28.7));
    assert_eq!(valid.optional_wbgt, Some(17.5));
    assert_eq!(valid.nullable_wbgt, Some(12.0));
    assert_eq!(valid.maximum_speed, Some(88));

    let sentinel: Reading = serde_json::from_str(
        r#"{"requiredWbgt":"-","optionalWbgt":"NA","nullableWbgt":"NA","maximumSpeed":"999"}"#,
    )
    .unwrap();
    assert_eq!(sentinel.required_wbgt, None);
    assert_eq!(sentinel.optional_wbgt, None);
    assert_eq!(sentinel.nullable_wbgt, None);
    assert_eq!(sentinel.maximum_speed, None);

    let null_and_missing: Reading =
        serde_json::from_str(r#"{"requiredWbgt":"28.7","nullableWbgt":null,"maximumSpeed":"88"}"#)
            .unwrap();
    assert_eq!(null_and_missing.optional_wbgt, None);
    assert_eq!(null_and_missing.nullable_wbgt, None);

    assert!(
        serde_json::from_str::<Reading>(
            r#"{"optionalWbgt":"1","nullableWbgt":"2","maximumSpeed":"88"}"#,
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<Reading>(
            r#"{"requiredWbgt":null,"nullableWbgt":"2","maximumSpeed":"88"}"#,
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<Reading>(
            r#"{"requiredWbgt":"unknown","nullableWbgt":"2","maximumSpeed":"88"}"#,
        )
        .is_err()
    );

    let encoded = serde_json::to_value(Reading {
        required_wbgt: None,
        optional_wbgt: None,
        nullable_wbgt: None,
        maximum_speed: None,
    })
    .unwrap();
    assert_eq!(
        encoded,
        serde_json::json!({
            "requiredWbgt": "NA",
            "nullableWbgt": "NA",
            "maximumSpeed": "999"
        })
    );
}
