use std::fs;

use crate::ast::*;
use crate::common::*;

const PROPERTY_IDENTIFIERS: &str =
    include_str!("../../../../tests/fixtures/property-identifiers.yaml");

#[test]
fn property_identifiers_render_with_rust_casing_and_wire_renames() {
    let files = satay_codegen::generate(PROPERTY_IDENTIFIERS)
        .expect("generate property identifier fixture");
    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let bus_stop = find_struct(&types_rs, "BusStop");

    assert_eq!(
        field_names(bus_stop),
        [
            "bus_stop_code",
            "road_name",
            "desc",
            "lat",
            "long",
            "request_id",
            "r#type",
        ]
    );
    for (rust_name, wire_name) in [
        ("desc", "Description"),
        ("lat", "Latitude"),
        ("long", "Longitude"),
        ("request_id", "RequestIdentifier"),
        ("r#type", "WireKeyword"),
    ] {
        assert_attr_contains(
            &field(bus_stop, rust_name).attrs,
            "cfg_attr",
            &format!(r#"rename = "{wire_name}""#),
        );
    }

    assert_field(bus_stop, "bus_stop_code", "u32");
    assert_field(bus_stop, "road_name", "String");
    assert_field(bus_stop, "desc", "String");
    assert_field(bus_stop, "lat", "f64");
    assert_field(bus_stop, "long", "f64");
    assert_field(bus_stop, "request_id", "String");
    assert_field(bus_stop, "r#type", "String");
}

#[test]
fn bus_stop_identifier_overrides_round_trip_with_original_wire_keys() {
    let files = satay_codegen::generate(PROPERTY_IDENTIFIERS)
        .expect("generate property identifier fixture");
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");
    let runtime_path = runtime_path_toml();
    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);

    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
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
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "property identifier generated crate tests",
    );
}
