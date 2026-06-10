use crate::ast::*;
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

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    assert_tuple_struct(&types_rs, "BusDirection", "u8");
    assert_tuple_struct(&types_rs, "BusLegacyDirection", "i32");
    let bus = find_struct(&types_rs, "Bus");
    assert_field(bus, "byte", "u8");
    assert_field(bus, "no_bounds", "i32");
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

    let parts_rs = parse_rust(find_file(&files, "get_buses/parts.rs"));
    let input = find_struct(&parts_rs, "GetBusesInput");
    assert_field(input, "skip", "Option<u64>");
    assert!(!contains_ident(&parts_rs, "GetBusesSkipParameter"));
    let types_rs = parse_rust(find_file(&files, "types.rs"));
    assert_tuple_struct(&types_rs, "GetBusesPageParameter", "i32");
    assert_field(input, "page", "Option<GetBusesPageParameter>");
}
