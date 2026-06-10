use crate::common::*;

#[test]
fn integer_bounds_infer_smallest_rust_type_and_can_be_overridden() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /buses:
    get:
      operationId: getBuses
      responses:
        '200':
          description: Buses
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Bus'
components:
  schemas:
    Bus:
      type: object
      required:
        - direction
        - byte
        - legacyDirection
        - noBounds
      properties:
        direction:
          type: integer
          format: int32
          minimum: 1
          maximum: 2
        byte:
          type: integer
          format: int64
          minimum: 0
          maximum: 255
        legacyDirection:
          type: integer
          format: int32
          minimum: 1
          maximum: 2
          x-satay:
            integer-type: i32
        noBounds:
          type: integer
          format: int32
"#,
    )
    .expect("generate integer inference fixture");

    let types_rs = find_file(&files, "types.rs");
    assert!(types_rs.contents.contains("pub struct BusDirection(u8);"));
    assert!(types_rs.contents.contains("pub byte: u8"));
    assert!(
        types_rs
            .contents
            .contains("pub struct BusLegacyDirection(i32);")
    );
    assert!(types_rs.contents.contains("pub no_bounds: i32"));
}

#[test]
fn open_ended_non_negative_unformatted_integer_parameters_use_unsigned() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /buses:
    get:
      operationId: getBuses
      parameters:
        - name: $skip
          in: query
          required: false
          schema:
            type: integer
            minimum: 0
        - name: page
          in: query
          required: false
          schema:
            type: integer
            format: int32
            minimum: 0
      responses:
        '204':
          description: No content
"#,
    )
    .expect("generate open-ended integer parameter fixture");

    let parts_rs = find_file(&files, "get_buses/parts.rs");
    assert!(parts_rs.contents.contains("pub skip: Option<u64>"));
    assert!(!parts_rs.contents.contains("GetBusesSkipParameter"));
    let types_rs = find_file(&files, "types.rs");
    assert!(
        types_rs
            .contents
            .contains("pub struct GetBusesPageParameter(i32);")
    );
    assert!(
        parts_rs
            .contents
            .contains("pub page: Option<GetBusesPageParameter>")
    );
}
