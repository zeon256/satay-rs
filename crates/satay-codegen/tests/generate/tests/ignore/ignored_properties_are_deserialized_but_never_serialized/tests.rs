use super::generated::BusArrivalResponse;

#[test]
fn ignored_wire_fields_are_lossy_on_round_trip() {
    let response: BusArrivalResponse = serde_json::from_str(
        r#"{
            "odata.metadata": "https://example.com/metadata",
            "nullableMetadata": null,
            "referencedMetadata": "https://example.com/referenced",
            "retainedMetadata": "kept",
            "BusStopCode": "83139",
            "Services": ["15"]
        }"#,
    )
    .unwrap();

    assert_eq!(response.bus_stop_code, "83139");
    assert_eq!(response.services, ["15"]);
    assert_eq!(response.retained_metadata.as_deref(), Some("kept"));

    let encoded = serde_json::to_value(response).unwrap();
    assert!(encoded.get("odata.metadata").is_none());
    assert!(encoded.get("nullableMetadata").is_none());
    assert!(encoded.get("referencedMetadata").is_none());
    assert_eq!(encoded["retainedMetadata"], "kept");
}

#[test]
fn ignored_required_fields_do_not_affect_rust_construction_or_decoding() {
    let constructed = BusArrivalResponse {
        bus_stop_code: "83139".to_owned(),
        services: vec![],
        retained_metadata: None,
    };
    assert_eq!(constructed.bus_stop_code, "83139");

    let decoded: BusArrivalResponse =
        serde_json::from_str(r#"{"BusStopCode":"83139","Services":[]}"#).unwrap();
    assert_eq!(decoded.bus_stop_code, "83139");
}
