use super::codegen::{self, Error, ValidationError};
use std::fs;

use super::common::*;

const COORDINATES: &str = r#"
openapi: 3.1.0
info:
  title: Coordinate codecs
  version: 1.0.0
paths: {}
components:
  schemas:
    Latitude:
      type: number
      format: double
      minimum: -90
      maximum: 90
    LatitudeAlias:
      $ref: '#/components/schemas/Latitude'
    Coordinates:
      type: object
      required: [Latitude, Longitude]
      properties:
        Latitude:
          $ref: '#/components/schemas/LatitudeAlias'
          x-satay:
            identifier: lat
        Longitude:
          type: number
          format: float
          minimum: -180
          maximum: 180
          x-satay:
            identifier: long
    CoordinatesAlias:
      $ref: '#/components/schemas/Coordinates'
    CoordinateString:
      type: string
      x-satay:
        parse-as: coordinates
        target: {$ref: '#/components/schemas/CoordinatesAlias'}
        fields: [Latitude, Longitude]
        delimiter: ','
    CoordinateStringAlias:
      $ref: '#/components/schemas/CoordinateString'
    PlainCoordinates:
      type: object
      required: [x, y]
      properties:
        x:
          type: number
          format: float
        y:
          type: number
          format: double
    PlainObservation:
      type: object
      required: [position]
      properties:
        position:
          type: string
          x-satay:
            parse-as: coordinates
            target: {$ref: '#/components/schemas/PlainCoordinates'}
            fields: [x, y]
            delimiter: '::'
    Observation:
      type: object
      required: [Label, Object, Aliased, Space, Reversed, Nullable, Sentinel, Lossy]
      properties:
        Label:
          type: string
        Object:
          $ref: '#/components/schemas/Coordinates'
        Aliased:
          $ref: '#/components/schemas/CoordinateStringAlias'
        Space:
          type: string
          x-satay:
            parse-as: coordinates
            target: {$ref: '#/components/schemas/Coordinates'}
            fields: [Latitude, Longitude]
        Reversed:
          type: string
          x-satay:
            parse-as: coordinates
            target: {$ref: '#/components/schemas/Coordinates'}
            fields: [Longitude, Latitude]
            delimiter: ','
        Nullable:
          type: [string, 'null']
          x-satay:
            parse-as: coordinates
            target: {$ref: '#/components/schemas/Coordinates'}
            fields: [Latitude, Longitude]
        Optional:
          type: string
          x-satay:
            parse-as: coordinates
            target: {$ref: '#/components/schemas/Coordinates'}
            fields: [Latitude, Longitude]
        Sentinel:
          type: string
          x-satay:
            parse-as: coordinates
            target: {$ref: '#/components/schemas/Coordinates'}
            fields: [Latitude, Longitude]
            none-if: ['', '-']
        Lossy:
          type: string
          x-satay:
            parse-as: coordinates
            target: {$ref: '#/components/schemas/Coordinates'}
            fields: [Latitude, Longitude]
            treat-error-as-none: true
"#;

#[test]
fn coordinate_fields_reuse_target_types_without_changing_object_serde() {
    let files = codegen::generate(COORDINATES).expect("generate coordinate codecs");
    let types = &find_file(&files, "types.rs").contents;
    // Keep coordinate helper calls within minimal_imports' two-segment limit.
    assert!(types.contains("use satay_runtime::serde_string::pair::option as pair_option;"));
    assert!(types.contains("pair_option::deserialize_lossy("));
    assert!(types.contains("pair_option::deserialize("));
    assert!(types.contains("use serde::de::Error;"));
    assert!(types.contains("use serde::ser::Error;"));
    assert!(!types.contains("pair::option::deserialize"));
    assert!(!types.contains("serde::de::Error::custom"));
    assert!(!types.contains("serde::ser::Error::custom"));
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    write_manifest(crate_dir, &runtime_path_toml(), true, false);
    write_generated_files(&crate_dir.join("src/generated"), &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/coordinates/coordinate_fields_reuse_target_types_without_changing_object_serde/tests.rs"
        ),
    );
    fs::write(crate_dir.join("src/lib.rs"), TEST_CRATE_LIB)
        .expect("write generated codec consumer");
    run_temp_cargo(crate_dir, "test", &[], "coordinate generated behavior");
    // Lossy decoding requires JSON, as it does for other parsed field types.
    let strict_spec = COORDINATES.replace("            treat-error-as-none: true", "");
    let strict_files = codegen::generate(&strict_spec).expect("generate serde-only codecs");
    write_generated_files(&crate_dir.join("src/generated"), &strict_files);
    run_temp_cargo(
        crate_dir,
        "check",
        &["--no-default-features", "--features", "serde"],
        "coordinate serde-only consumer",
    );
    run_temp_cargo(
        crate_dir,
        "check",
        &["--no-default-features"],
        "coordinate consumer without serde",
    );
}

#[test]
fn coordinate_model_names_do_not_collide_with_serde_helper_generics() {
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    write_manifest(crate_dir, &runtime_path_toml(), true, false);
    fs::write(crate_dir.join("src/lib.rs"), "pub mod generated;\n")
        .expect("write generated codec consumer");

    for (target, component) in [("D", "Serializer"), ("Serializer", "Latitude")] {
        let spec = COORDINATES
            .replace("Coordinates", target)
            .replace("Latitude", component);
        let files = codegen::generate(&spec).expect("generate colliding coordinate names");
        assert!(
            find_file(&files, "types.rs")
                .contents
                .contains(&format!("self::{component}::try_new"))
        );
        write_generated_files(&crate_dir.join("src/generated"), &files);
        run_temp_cargo(crate_dir, "check", &[], "coordinate helper name collisions");
    }
}

#[test]
fn coordinate_constrained_constructor_is_qualified() {
    let spec = COORDINATES.replace("Latitude", "D");
    let files = codegen::generate(&spec).expect("generate constrained component named D");
    // nutype's Deserialize derive has its own collision for a type declared as D,
    // so verify the coordinate constructor independently of that macro expansion.
    assert!(
        find_file(&files, "types.rs")
            .contents
            .contains("self::D::try_new")
    );
}

#[test]
fn coordinate_configuration_rejects_ambiguous_or_unconstructible_targets() {
    for (old, replacement) in [
        (
            "fields: [Latitude, Longitude]",
            "fields: [Latitude, Latitude]",
        ),
        ("fields: [Latitude, Longitude]", "fields: [Latitude]"),
        (
            "fields: [Latitude, Longitude]",
            "fields: [Latitude, Missing]",
        ),
        ("delimiter: ','", "delimiter: ''"),
        ("required: [Latitude, Longitude]", "required: [Latitude]"),
        (
            "type: number\n      format: double",
            "type: [number, 'null']\n      format: double",
        ),
        (
            "target: {$ref: '#/components/schemas/Coordinates'}",
            "target: {$ref: '#/components/schemas/Latitude'}",
        ),
        (
            "target: {$ref: '#/components/schemas/Coordinates'}",
            "target: {$ref: '#/components/schemas/Missing'}",
        ),
        (
            "$ref: '#/components/schemas/LatitudeAlias'",
            "$ref: '#/components/schemas/Coordinates'",
        ),
        ("parse-as: coordinates", "parse-as: f64"),
    ] {
        let spec = COORDINATES.replace(old, replacement);
        assert!(codegen::generate(&spec).is_err(), "accepted {replacement}");
    }
}

#[test]
fn coordinate_aliases_cannot_bypass_field_local_decoding() {
    use serde_json::json;

    let reference = json!({"$ref": "#/components/schemas/CoordinateStringAlias"});
    for body in [
        reference.clone(),
        json!({"type": "array", "items": reference}),
        json!({"type": "object", "additionalProperties": reference}),
        json!({"anyOf": [reference, {"type": "boolean"}]}),
    ] {
        let mut spec = serde_json::to_value(oas3::from_yaml(COORDINATES).unwrap()).unwrap();
        spec["paths"] = json!({
            "/location": {"get": {
                "operationId": "location",
                "responses": {"200": {
                    "description": "Location",
                    "content": {"application/json": {"schema": body}}
                }}
            }}
        });
        assert!(matches!(
            codegen::generate(&spec.to_string()),
            Err(Error::Validation(
                ValidationError::SatayCoordinatesRequireStructField { .. }
            ))
        ));
    }
}
