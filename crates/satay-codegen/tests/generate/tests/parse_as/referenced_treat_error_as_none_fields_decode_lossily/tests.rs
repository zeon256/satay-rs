use super::generated::BusServiceArrival;

#[test]
fn valid_nested_bus_is_some_and_invalid_buses_are_none() {
    let service: BusServiceArrival = serde_json::from_str(
        r#"{
            "NextBus": {
                "OriginCode": "12345",
                "EstimatedArrival": "2024-08-14T16:41:48+08:00"
            },
            "NextBus2": {},
            "NextBus3": {
                "OriginCode": "",
                "EstimatedArrival": ""
            }
        }"#,
    )
    .unwrap();

    assert!(service.next_bus.is_some());
    assert_eq!(service.next_bus.as_ref().unwrap().origin_code, 12345);
    assert_eq!(service.next_bus2, None);
    assert_eq!(service.next_bus3, None);

    let encoded = serde_json::to_value(service).unwrap();
    assert!(encoded.get("NextBus").is_some());
    assert!(encoded.get("NextBus2").is_none());
    assert!(encoded.get("NextBus3").is_none());
}
