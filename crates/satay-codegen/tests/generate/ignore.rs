use std::fs;

use crate::ast::*;
use crate::common::*;

#[test]
fn ignored_properties_are_deserialized_but_never_serialized() {
    let files = satay_codegen::generate(
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
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
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

        let decoded: BusArrivalResponse = serde_json::from_str(
            r#"{"BusStopCode":"83139","Services":[]}"#,
        )
        .unwrap();
        assert_eq!(decoded.bus_stop_code, "83139");
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "ignored property generated crate tests",
    );
}
