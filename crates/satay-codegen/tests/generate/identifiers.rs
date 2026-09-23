use super::codegen;
use std::fs;

use super::ast::*;
use super::common::*;

const PROPERTY_IDENTIFIERS: &str =
    include_str!("../../../../tests/fixtures/property-identifiers.yaml");

#[test]
fn property_identifiers_render_with_rust_casing_and_wire_renames() {
    let files =
        codegen::generate(PROPERTY_IDENTIFIERS).expect("generate property identifier fixture");
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
    assert_field(
        bus_stop,
        "road_name",
        "<S as satay_runtime::storage::Storage>::Text<'storage>",
    );
    assert_field(
        bus_stop,
        "desc",
        "<S as satay_runtime::storage::Storage>::Text<'storage>",
    );
    assert_field(bus_stop, "lat", "f64");
    assert_field(bus_stop, "long", "f64");
    assert_field(
        bus_stop,
        "request_id",
        "<S as satay_runtime::storage::Storage>::Text<'storage>",
    );
    assert_field(
        bus_stop,
        "r#type",
        "<S as satay_runtime::storage::Storage>::Text<'storage>",
    );
}

#[test]
fn bus_stop_identifier_overrides_round_trip_with_original_wire_keys() {
    let files =
        codegen::generate(PROPERTY_IDENTIFIERS).expect("generate property identifier fixture");
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");
    let runtime_path = runtime_path_toml();
    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);

    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/identifiers/bus_stop_identifier_overrides_round_trip_with_original_wire_keys/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "property identifier generated crate tests",
    );
}
