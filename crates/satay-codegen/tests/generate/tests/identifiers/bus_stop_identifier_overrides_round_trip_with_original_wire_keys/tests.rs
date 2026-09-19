use super::generated::BusStop;

#[test]
fn public_identifiers_are_independent_of_wire_names() {
    let bus_stop: BusStop = serde_json::from_str(
        r#"{
            "BusStopCode": "83139",
            "RoadName": "Bencoolen St",
            "Description": "Bef Bencoolen Stn Exit B",
            "Latitude": 1.299604,
            "Longitude": 103.850604,
            "RequestIdentifier": "request-7",
            "WireKeyword": "keyword"
        }"#,
    )
    .unwrap();

    assert_eq!(bus_stop.bus_stop_code, 83139);
    assert_eq!(bus_stop.road_name, "Bencoolen St");
    assert_eq!(bus_stop.desc, "Bef Bencoolen Stn Exit B");
    assert_eq!(bus_stop.lat, 1.299604);
    assert_eq!(bus_stop.long, 103.850604);
    assert_eq!(bus_stop.request_id, "request-7");
    assert_eq!(bus_stop.r#type, "keyword");

    let encoded = serde_json::to_value(bus_stop).unwrap();
    assert_eq!(encoded["Description"], "Bef Bencoolen Stn Exit B");
    assert_eq!(encoded["Latitude"], 1.299604);
    assert_eq!(encoded["Longitude"], 103.850604);
    assert_eq!(encoded["RequestIdentifier"], "request-7");
    assert_eq!(encoded["WireKeyword"], "keyword");
    assert!(encoded.get("desc").is_none());
    assert!(encoded.get("request_id").is_none());
}
