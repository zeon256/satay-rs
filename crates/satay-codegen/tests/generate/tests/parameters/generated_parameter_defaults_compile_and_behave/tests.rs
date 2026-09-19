use super::generated::*;

#[test]
fn omitted_values_use_defaults() {
    let parts = operations::get_parking::get_parking_parts(GetParkingInput::<String>::new())
        .expect("request parts");

    assert_eq!(
        parts.uri,
        "/parking?Dist=0.5&Limit=25&Ratio=0.25&Mode=rack&Empty-Mode="
    );
    assert_eq!(parts.headers.get("X-Region").unwrap(), "central");
}

#[test]
fn default_impl_uses_parameter_defaults() {
    let parts = operations::get_parking::get_parking_parts(GetParkingInput::<String>::default())
        .expect("request parts");

    assert_eq!(
        parts.uri,
        "/parking?Dist=0.5&Limit=25&Ratio=0.25&Mode=rack&Empty-Mode="
    );
    assert_eq!(parts.headers.get("X-Region").unwrap(), "central");
}

#[test]
fn explicit_values_override_defaults_and_absent_parameters_stay_absent() {
    let parts = operations::get_parking::get_parking_parts(
        GetParkingInput::<String>::new()
            .dist(1.25)
            .limit(50)
            .ratio(0.75)
            .mode(ParkingMode::Lot)
            .x_region("west")
            .filter("covered"),
    )
    .expect("request parts");

    assert_eq!(
        parts.uri,
        "/parking?Dist=1.25&Limit=50&Ratio=0.75&Mode=lot&Empty-Mode=&Filter=covered"
    );
    assert_eq!(parts.headers.get("X-Region").unwrap(), "west");

    let omitted = operations::get_parking::get_parking_parts(GetParkingInput::<String>::new())
        .expect("request parts");
    assert!(!omitted.uri.contains("Filter="));
}
