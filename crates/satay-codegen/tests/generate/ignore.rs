use super::codegen;
use std::fs;

use super::ast::*;
use super::common::*;

#[test]
fn ignored_properties_are_deserialized_but_never_serialized() {
    let files = codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Bus Arrival API
  version: 1.0.0
paths: {}
components:
  schemas:
    MetadataUri:
      type: string
      format: uri
    BusArrivalResponse:
      type: object
      additionalProperties: false
      required: [odata.metadata, nullableMetadata, referencedMetadata, BusStopCode, Services]
      properties:
        odata.metadata:
          type: string
          format: uri
          x-satay:
            ignore: true
        nullableMetadata:
          type: [string, "null"]
          x-satay:
            ignore: true
        referencedMetadata:
          $ref: '#/components/schemas/MetadataUri'
          x-satay:
            ignore: true
        retainedMetadata:
          type: string
        BusStopCode:
          type: string
        Services:
          type: array
          items:
            type: string
"#,
    )
    .expect("generate ignored property fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let response = find_struct(&types_rs, "BusArrivalResponse");
    assert_eq!(
        field_names(response),
        ["retained_metadata", "bus_stop_code", "services"]
    );

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");
    let runtime_path = runtime_path_toml();
    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/ignore/ignored_properties_are_deserialized_but_never_serialized/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "ignored property generated crate tests",
    );
}
