use super::generated::*;

#[test]
fn constructs_actions_through_each_group() {
    let api = Api::new();

    let bus = api
        .bus()
        .get_arrival(83139)
        .service_no("15")
        .include_metadata(true)
        .request()
        .unwrap();
    assert_eq!(
        bus.uri(),
        "/arrival?busStopCode=83139&serviceNo=15&includeMetadata=true"
    );

    let realtime = api.realtime().get_bus_arrival(83139).request().unwrap();
    assert_eq!(realtime.uri(), "/arrival?busStopCode=83139");

    let health = api.untagged().health().request().unwrap();
    assert_eq!(health.uri(), "/health");
}
