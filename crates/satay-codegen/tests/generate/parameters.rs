use std::fs;

use satay_codegen::{Error, ValidationError};

use crate::common::*;

const PARAMETER_DEFAULTS: &str = r#"
openapi: 3.1.0
info:
  title: Parameter defaults
  version: 1.0.0
paths:
  /parking:
    get:
      operationId: getParking
      parameters:
        - name: Dist
          in: query
          schema:
            type: number
            format: double
            default: 0.5
        - name: Limit
          in: query
          schema:
            type: integer
            format: int32
            default: 25
        - name: Ratio
          in: query
          schema:
            type: number
            format: float
            default: 0.25
        - name: Mode
          in: query
          schema:
            $ref: '#/components/schemas/ParkingMode'
        - name: Empty-Mode
          in: query
          schema:
            type: string
            enum: ['']
            default: ''
        - name: X-Region
          in: header
          schema:
            type: string
            default: central
        - name: Filter
          in: query
          schema:
            type: string
      responses:
        '204':
          description: No content
  /required:
    get:
      operationId: getRequired
      parameters:
        - name: Tags
          in: query
          required: true
          schema:
            type: array
            items:
              type: string
            default: [covered]
      responses:
        '204':
          description: No content
components:
  schemas:
    ParkingMode:
      type: string
      enum: [rack, lot]
      default: rack
"#;

#[test]
fn generated_parameter_defaults_compile_and_behave() {
    let files = satay_codegen::generate(PARAMETER_DEFAULTS).expect("generate defaults fixture");
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    write_manifest(crate_dir, &runtime_path_toml(), false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r#"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn omitted_values_use_defaults() {
        let parts = operations::get_parking::get_parking_parts(GetParkingInput::new())
            .expect("request parts");

        assert_eq!(
            parts.uri,
            "/parking?Dist=0.5&Limit=25&Ratio=0.25&Mode=rack&Empty-Mode="
        );
        assert_eq!(parts.headers.get("X-Region").unwrap(), "central");
    }

    #[test]
    fn default_impl_uses_parameter_defaults() {
        let parts = operations::get_parking::get_parking_parts(GetParkingInput::default())
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
            GetParkingInput::new()
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

        let omitted = operations::get_parking::get_parking_parts(GetParkingInput::new())
            .expect("request parts");
        assert!(!omitted.uri.contains("Filter="));
    }
}
"#;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "generated parameter default tests");
}

#[test]
fn rejects_invalid_parameter_defaults() {
    let cases = [
        (
            "type: number\n            default: nope",
            "expected a finite JSON number",
        ),
        (
            "type: integer\n            default: 1.5",
            "expected a JSON integer",
        ),
        (
            "type: integer\n            format: int32\n            default: 2147483648",
            "outside the generated integer range",
        ),
        (
            "type: string\n            enum: [rack, lot]\n            default: street",
            "not a declared enum variant",
        ),
        (
            "type: string\n            pattern: '^[a-z]+$'\n            default: UPPER",
            "does not match pattern",
        ),
        (
            "type: array\n            items:\n              type: string\n            default: [rack]",
            "array parameter defaults are not supported",
        ),
        (
            "type: number\n            format: float\n            exclusiveMinimum: 0.1\n            default: 0.100000001",
            "below the schema minimum",
        ),
        (
            "type: integer\n            format: int64\n            default: 9007199254740993.0",
            "expected a JSON integer",
        ),
        (
            "type: string\n            enum: [a, bb]\n            minLength: 2\n            default: a",
            "below minLength",
        ),
        (
            "type: string\n            enum: ['', ready]\n            default: ''",
            "cannot be represented by the generated parameter enum",
        ),
    ];

    for (schema, expected_reason) in cases {
        let spec = format!(
            r#"
openapi: 3.1.0
info:
  title: Invalid parameter default
  version: 1.0.0
paths:
  /parking:
    get:
      operationId: getParking
      parameters:
        - name: Value
          in: query
          schema:
            {schema}
      responses:
        '204':
          description: No content
"#
        );

        let err = satay_codegen::generate(&spec).expect_err("invalid default must be rejected");
        let Error::Validation(ValidationError::InvalidParameterDefault {
            wire_name,
            value: _,
            reason,
        }) = err
        else {
            panic!("unexpected error: {err}");
        };
        assert_eq!(wire_name, "Value");
        assert!(
            reason.contains(expected_reason),
            "expected reason containing {expected_reason:?}, got {reason:?}"
        );
    }
}
