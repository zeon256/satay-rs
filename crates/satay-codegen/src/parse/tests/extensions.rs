use super::*;

#[test]
fn parses_x_satay_parse_as_for_string_schemas() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    Arrival:
      type: object
      required:
        - stop
        - latitude
        - visit
        - monitored
        - numericMonitored
        - estimatedArrival
        - frequency
        - ratio
      properties:
        stop:
          type: string
          minLength: 1
          x-satay:
            parse-as: u32
        latitude:
          type: string
          x-satay:
            parse-as: f64
        visit:
          type: string
          x-satay:
            parse-as: u8
        monitored:
          type: string
          x-satay:
            parse-as: bool
        numericMonitored:
          type: integer
          x-satay:
            parse-as: bool
        estimatedArrival:
          type: string
          x-satay:
            parse-as: offset-datetime
        frequency:
          type: string
          minimum: 1
          maximum: 60
          x-satay:
            parse-as: integer-range
        ratio:
          type: string
          format: float
          x-satay:
            parse-as: number-range
"#,
    );

    let arrival = component(&api, "Arrival");
    match &arrival.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(
                field(fields, "stop").ty,
                TypeRef::ParsedString(ParseAs::U32)
            );
            assert_eq!(
                field(fields, "latitude").ty,
                TypeRef::ParsedString(ParseAs::F64)
            );
            assert_eq!(
                field(fields, "visit").ty,
                TypeRef::ParsedString(ParseAs::U8)
            );
            assert_eq!(
                field(fields, "monitored").ty,
                TypeRef::ParsedString(ParseAs::Bool)
            );
            assert_eq!(
                field(fields, "numericMonitored").ty,
                TypeRef::ParsedInteger(ParseAs::Bool)
            );
            assert_eq!(
                field(fields, "estimatedArrival").ty,
                TypeRef::ParsedString(ParseAs::OffsetDateTime)
            );
            assert_eq!(
                field(fields, "frequency").ty,
                TypeRef::Range(RangeTypeRef {
                    rust_name: "ArrivalFrequency".to_owned(),
                    scalar: RangeScalar::Integer(IntegerType::U8),
                })
            );
            assert_eq!(
                field(fields, "ratio").ty,
                TypeRef::Range(RangeTypeRef {
                    rust_name: "ArrivalRatio".to_owned(),
                    scalar: RangeScalar::F32,
                })
            );
        }
        other => panic!("expected Arrival struct, got {other:?}"),
    }
}

#[test]
fn lowers_date_parse_as_on_query_parameters() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /psi:
    get:
      operationId: psi
      parameters:
        - name: date
          in: query
          schema:
            type: string
            x-satay:
              parse-as: date
      responses:
        '204':
          description: No content
"#,
    );

    let date = parameter(&api.operations[0], "date");
    assert_eq!(date.ty, TypeRef::ParsedString(ParseAs::Date));
    assert!(!date.required);
}

#[test]
fn lowers_naive_datetime_parse_as_on_query_parameters() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /psi:
    get:
      operationId: psi
      parameters:
        - name: date
          in: query
          schema:
            type: string
            x-satay:
              parse-as: naive-datetime
      responses:
        '204':
          description: No content
"#,
    );

    let date = parameter(&api.operations[0], "date");
    assert_eq!(date.ty, TypeRef::ParsedString(ParseAs::NaiveDateTime));
    assert!(!date.required);
}

#[test]
fn parses_x_satay_enum_variants() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    VehicleType:
      type: string
      enum:
        - SD
        - DD
        - BD
        - ""
      x-satay:
        enum-variants:
          SD: SingleDecker
          DD: DoubleDecker
          BD: Bendy
          "": Unknown
    Arrival:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          enum:
            - SD
            - DD
            - BD
            - ""
          x-satay:
            enum-variants:
              SD: SingleDecker
              DD: DoubleDecker
              BD: Bendy
              "": Unknown
"#,
    );

    let vehicle_type = component(&api, "VehicleType");
    match &vehicle_type.kind {
        ComponentKind::Enum(variants) => {
            assert_eq!(variants.len(), 3);
            assert_eq!(variants[0].wire_name, "SD");
            assert_eq!(variants[0].rust_name, "SingleDecker");
            assert_eq!(variants[1].wire_name, "DD");
            assert_eq!(variants[1].rust_name, "DoubleDecker");
            assert_eq!(variants[2].wire_name, "BD");
            assert_eq!(variants[2].rust_name, "Bendy");
        }
        other => panic!("expected VehicleType enum, got {other:?}"),
    }

    let arrival_type = component(&api, "ArrivalType");
    match &arrival_type.kind {
        ComponentKind::Enum(variants) => {
            assert_eq!(variants.len(), 3);
            assert_eq!(variants[0].rust_name, "SingleDecker");
            assert_eq!(variants[1].rust_name, "DoubleDecker");
            assert_eq!(variants[2].rust_name, "Bendy");
        }
        other => panic!("expected ArrivalType enum, got {other:?}"),
    }
}

#[test]
fn rejects_x_satay_enum_variants_for_values_outside_enum() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/VehicleType'
components:
  schemas:
    VehicleType:
      type: string
      enum:
        - SD
      x-satay:
        enum-variants:
          DD: DoubleDecker
"#,
    );

    match err {
        ValidationError::UnknownSatayEnumVariantValue { context, wire_name } => {
            assert_eq!(context, "schema `VehicleType`");
            assert_eq!(wire_name, "DD");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn parses_x_satay_treat_error_as_none() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    Arrival:
      type: object
      required:
        - timing
      properties:
        timing:
          type: string
          x-satay:
            treat-error-as-none: true
        optionalTiming:
          type: string
"#,
    );

    let arrival = component(&api, "Arrival");
    match &arrival.kind {
        ComponentKind::Struct(fields) => {
            let timing = field(fields, "timing");
            assert!(timing.treat_error_as_none);
            let optional_timing = field(fields, "optionalTiming");
            assert!(!optional_timing.treat_error_as_none);
        }
        other => panic!("expected Arrival struct, got {other:?}"),
    }
}

#[test]
fn validates_x_satay_parse_as_on_reachable_operation_schemas() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      parameters:
        - name: includeDetails
          in: query
          schema:
            type: boolean
            x-satay:
              parse-as: u8
      responses:
        '204':
          description: No content
"#,
    );

    match err {
        ValidationError::SatayParseAsRequiresString {
            context,
            parse_as,
            kind,
        } => {
            assert_eq!(context, "parameter `includeDetails`");
            assert_eq!(parse_as, "u8");
            assert_eq!(kind, "boolean");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn validates_x_satay_integer_type_on_reachable_request_body_schema() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    post:
      operationId: createArrival
      requestBody:
        content:
          application/json:
            schema:
              type: string
              x-satay:
                integer-type: u8
      responses:
        '204':
          description: No content
"#,
    );

    match err {
        ValidationError::SatayIntegerTypeRequiresInteger {
            context,
            integer_type,
            kind,
        } => {
            assert_eq!(context, "operation `createArrival` requestBody");
            assert_eq!(integer_type, "u8");
            assert_eq!(kind, "string");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn validates_x_satay_treat_error_as_none_on_struct_properties() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    Arrival:
      type: object
      properties:
        timing:
          type: string
          x-satay:
            treat-error-as-none: yes
"#,
    );

    match err {
        ValidationError::InvalidBooleanKeyword { context, keyword } => {
            assert_eq!(context, "property `Arrival.timing`");
            assert_eq!(keyword, "treat-error-as-none");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn skips_x_satay_validation_for_unreachable_component_parameters() {
    parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  parameters:
    BrokenButUnused:
      name: includeDetails
      in: query
      schema:
        type: boolean
        x-satay:
          parse-as: u8
"#,
    );
}
