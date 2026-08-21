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
                TypeRef::ParsedString(StringCodec::Standard(ParseAs::U32))
            );
            assert_eq!(
                field(fields, "latitude").ty,
                TypeRef::ParsedString(StringCodec::Standard(ParseAs::F64))
            );
            assert_eq!(
                field(fields, "visit").ty,
                TypeRef::ParsedString(StringCodec::Standard(ParseAs::U8))
            );
            assert_eq!(
                field(fields, "monitored").ty,
                TypeRef::ParsedString(StringCodec::Standard(ParseAs::Bool))
            );
            assert_eq!(
                field(fields, "numericMonitored").ty,
                TypeRef::ParsedInteger(ParseAs::Bool)
            );
            assert_eq!(
                field(fields, "estimatedArrival").ty,
                TypeRef::ParsedString(StringCodec::Standard(ParseAs::OffsetDateTime))
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
    assert_eq!(
        date.ty,
        TypeRef::ParsedString(StringCodec::Standard(ParseAs::Date))
    );
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
    assert_eq!(
        date.ty,
        TypeRef::ParsedString(StringCodec::Standard(ParseAs::NaiveDateTime))
    );
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
        ComponentKind::Enum(enum_) => {
            let variants = &enum_.variants;
            assert_eq!(variants.len(), 4);
            assert_eq!(variants[0].wire_name, "SD");
            assert_eq!(variants[0].rust_name, "SingleDecker");
            assert_eq!(variants[1].wire_name, "DD");
            assert_eq!(variants[1].rust_name, "DoubleDecker");
            assert_eq!(variants[2].wire_name, "BD");
            assert_eq!(variants[2].rust_name, "Bendy");
            assert_eq!(variants[3].wire_name, "");
            assert_eq!(variants[3].rust_name, "Unknown");
            assert_eq!(enum_.fallback, EnumFallback::None);
        }
        other => panic!("expected VehicleType enum, got {other:?}"),
    }

    let arrival_type = component(&api, "ArrivalType");
    match &arrival_type.kind {
        ComponentKind::Enum(enum_) => {
            let variants = &enum_.variants;
            assert_eq!(variants.len(), 4);
            assert_eq!(variants[0].rust_name, "SingleDecker");
            assert_eq!(variants[1].rust_name, "DoubleDecker");
            assert_eq!(variants[2].rust_name, "Bendy");
            assert_eq!(variants[3].rust_name, "Unknown");
            assert_eq!(enum_.fallback, EnumFallback::None);
        }
        other => panic!("expected ArrivalType enum, got {other:?}"),
    }
}

#[test]
fn parses_x_satay_enum_variants_using_other_for_closed_enum() {
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
                $ref: '#/components/schemas/VehicleType'
components:
  schemas:
    VehicleType:
      type: string
      enum:
        - SD
      x-satay:
        enum-variants:
          SD: Other
"#,
    );

    let vehicle_type = component(&api, "VehicleType");
    match &vehicle_type.kind {
        ComponentKind::Enum(enum_) => {
            assert_eq!(enum_.variants.len(), 1);
            assert_eq!(enum_.variants[0].wire_name, "SD");
            assert_eq!(enum_.variants[0].rust_name, "Other");
            assert_eq!(enum_.fallback, EnumFallback::None);
        }
        other => panic!("expected VehicleType enum, got {other:?}"),
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
fn rejects_x_satay_enum_variants_without_enum_values() {
    for (spec, expected_context) in [
        (
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    VehicleType:
      type: string
      x-satay:
        enum-variants: {}
"#,
            "schema `VehicleType`",
        ),
        (
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Arrival:
      type: object
      properties:
        vehicle:
          type: string
          x-satay:
            enum-variants:
              SD: SingleDecker
"#,
            "property `Arrival.vehicle`",
        ),
    ] {
        let err = parse_invalid(spec);

        assert!(matches!(
            err,
            ValidationError::SatayEnumVariantsRequireEnum { context }
                if context == expected_context
        ));
    }
}

#[test]
fn rejects_x_satay_parse_as_with_enum_values() {
    for (schema, expected_parse_as) in [
        (
            r#"
          type: string
          enum:
            - SD
          x-satay:
            parse-as: date
"#,
            "date",
        ),
        (
            r#"
          type: integer
          enum:
            - 1
            - 0
          x-satay:
            parse-as: bool
"#,
            "bool",
        ),
    ] {
        let err = parse_invalid(&format!(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {{}}
components:
  schemas:
    Arrival:
      type: object
      properties:
        vehicle:
{schema}
"#
        ));

        match err {
            ValidationError::SatayParseAsWithEnum { context, parse_as } => {
                assert_eq!(context, "property `Arrival.vehicle`");
                assert_eq!(parse_as, expected_parse_as);
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}

#[test]
fn rejects_x_satay_enum_variants_using_reserved_fallback_names() {
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
      anyOf:
        - type: string
        - type: string
          enum:
            - SD
          x-satay:
            enum-variants:
              SD: Other
"#,
    );

    match err {
        ValidationError::ReservedSatayEnumVariantName {
            context,
            wire_name,
            rust_name,
        } => {
            assert_eq!(context, "schema `VehicleType`");
            assert_eq!(wire_name, "SD");
            assert_eq!(rust_name, "Other");
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
fn parses_x_satay_treat_error_as_none_on_required_reference_property() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    BusArrivalTiming:
      type: object
      required: [estimatedArrival]
      properties:
        estimatedArrival:
          type: string
    BusServiceArrival:
      type: object
      required: [nextBus, strictNextBus]
      properties:
        nextBus:
          $ref: '#/components/schemas/BusArrivalTiming'
          x-satay:
            treat-error-as-none: true
        strictNextBus:
          $ref: '#/components/schemas/BusArrivalTiming'
          x-satay:
            treat-error-as-none: false
"#,
    );

    let arrival = component(&api, "BusServiceArrival");
    let ComponentKind::Struct(fields) = &arrival.kind else {
        panic!("expected BusServiceArrival struct");
    };
    let next_bus = field(fields, "nextBus");
    assert!(next_bus.required);
    assert!(next_bus.treat_error_as_none);
    assert_eq!(next_bus.ty, TypeRef::Named("BusArrivalTiming".to_owned()));
    let strict_next_bus = field(fields, "strictNextBus");
    assert!(strict_next_bus.required);
    assert!(!strict_next_bus.treat_error_as_none);
    assert_eq!(
        strict_next_bus.ty,
        TypeRef::Named("BusArrivalTiming".to_owned())
    );
}

#[test]
fn rejects_property_only_options_on_value_enum_schemas_by_presence() {
    for value in [true, false] {
        let spec = format!(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {{}}
components:
  schemas:
    Status:
      type: string
      enum: [ready]
      x-satay:
        treat-error-as-none: {value}
"#
        );

        assert!(matches!(
            parse_invalid(&spec),
            ValidationError::SatayTreatErrorAsNoneRequiresObjectProperty { context }
                if context == "schema `Status`"
        ));
    }
}

#[test]
fn rejects_property_only_options_on_open_enum_value_branches() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Status:
      anyOf:
        - type: string
        - type: string
          enum: [ready]
          x-satay:
            treat-error-as-none: false
"#,
    );

    assert!(matches!(
        err,
        ValidationError::SatayTreatErrorAsNoneRequiresObjectProperty { context }
            if context == "schema `Status`"
    ));
}

#[test]
fn validates_array_items_in_value_context() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Record:
      type: object
      properties:
        values:
          type: array
          items:
            type: string
            x-satay:
              treat-error-as-none: false
"#,
    );

    assert!(matches!(
        err,
        ValidationError::SatayTreatErrorAsNoneRequiresObjectProperty { context }
            if context == "property `Record.values` items"
    ));
}

#[test]
fn rejects_property_options_on_value_reference_siblings_by_presence() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Identifier:
      type: string
    IdentifierAlias:
      $ref: '#/components/schemas/Identifier'
      x-satay:
        treat-error-as-none: false
"#,
    );

    assert!(matches!(
        err,
        ValidationError::UnsupportedRefSiblingKeyword { context, keyword }
            if context == "schema `IdentifierAlias`"
                && keyword == "x-satay.treat-error-as-none"
    ));
}

#[test]
fn omits_x_satay_ignored_object_properties() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
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
      required: [odata.metadata, nullableMetadata, referencedMetadata, BusStopCode]
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
          x-satay:
            ignore: false
        BusStopCode:
          type: string
"#,
    );

    let response = component(&api, "BusArrivalResponse");
    let ComponentKind::Struct(fields) = &response.kind else {
        panic!("expected BusArrivalResponse struct");
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(field(fields, "retainedMetadata").ty, TypeRef::String);
    assert_eq!(field(fields, "BusStopCode").ty, TypeRef::String);
}

#[test]
fn rejects_x_satay_ignore_outside_object_properties() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    MetadataUri:
      type: string
      x-satay:
        ignore: true
"#,
    );

    assert!(matches!(
        err,
        ValidationError::SatayIgnoreRequiresObjectProperty { context }
            if context == "schema `MetadataUri`"
    ));
}

#[test]
fn rejects_non_boolean_x_satay_ignore() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Response:
      type: object
      properties:
        metadata:
          type: string
          x-satay:
            ignore: yes
"#,
    );

    assert!(matches!(
        err,
        ValidationError::InvalidExtension { context, path, .. }
            if context == "property `Response.metadata`" && path == "x-satay.ignore"
    ));
}

#[test]
fn validates_ignored_property_schemas() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Response:
      type: object
      properties:
        metadata:
          type: boolean
          x-satay:
            ignore: true
            parse-as: u32
"#,
    );

    assert!(matches!(
        err,
        ValidationError::SatayParseAsRequiresString { context, .. }
            if context == "property `Response.metadata`"
    ));
}

#[test]
fn ignored_properties_do_not_participate_in_identifier_collisions() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Record:
      type: object
      properties:
        value:
          type: string
          x-satay:
            ignore: true
        display:
          type: string
          x-satay:
            identifier: value
"#,
    );

    let record = component(&api, "Record");
    let ComponentKind::Struct(fields) = &record.kind else {
        panic!("expected Record struct");
    };
    assert_eq!(fields.len(), 1);
    assert_eq!(field(fields, "display").rust_name, "value");
}

#[test]
fn parses_target_neutral_property_identifiers_into_ir() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Identifier:
      type: string
    BusStop:
      type: object
      properties:
        Description:
          type: string
          x-satay:
            identifier: desc
        RequestIdentifier:
          type: string
          x-satay:
            identifier: request-id
        ReferencedIdentifier:
          $ref: '#/components/schemas/Identifier'
          x-satay:
            identifier: reference-id
        RoadName:
          type: string
        WireKeyword:
          type: string
          x-satay:
            identifier: type
"#,
    );

    let bus_stop = component(&api, "BusStop");
    let ComponentKind::Struct(fields) = &bus_stop.kind else {
        panic!("expected BusStop struct");
    };

    assert_eq!(field(fields, "Description").rust_name, "desc");
    assert_eq!(
        field(fields, "Description").identifier_words.as_deref(),
        Some(["desc".to_owned()].as_slice())
    );
    assert_eq!(field(fields, "RequestIdentifier").rust_name, "request_id");
    assert_eq!(
        field(fields, "RequestIdentifier")
            .identifier_words
            .as_deref(),
        Some(["request".to_owned(), "id".to_owned()].as_slice())
    );
    assert_eq!(
        field(fields, "ReferencedIdentifier").rust_name,
        "reference_id"
    );
    assert_eq!(field(fields, "WireKeyword").rust_name, "r#type");
    assert_eq!(field(fields, "RoadName").rust_name, "road_name");
    assert!(field(fields, "RoadName").identifier_words.is_none());
}

#[test]
fn rejects_invalid_target_neutral_property_identifiers() {
    let cases = [
        (r#""""#, "identifier must not be empty"),
        ("RequestId", "lower kebab-case"),
        ("request_id", "lower kebab-case"),
        ("-request", "lower kebab-case"),
        ("request--id", "lower kebab-case"),
        ("request-id-", "lower kebab-case"),
        ("request.id", "lower kebab-case"),
    ];

    for (identifier, expected_message) in cases {
        let spec = format!(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {{}}
components:
  schemas:
    Record:
      type: object
      properties:
        RequestIdentifier:
          type: string
          x-satay:
            identifier: {identifier}
"#
        );

        match parse_invalid(&spec) {
            ValidationError::InvalidExtension {
                context,
                path,
                source,
            } => {
                assert_eq!(context, "property `Record.RequestIdentifier`");
                assert_eq!(path, "x-satay.identifier");
                assert!(
                    source.to_string().contains(expected_message),
                    "unexpected diagnostic: {source}"
                );
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}

#[test]
fn rejects_property_identifier_outside_object_properties() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Description:
      type: string
      x-satay:
        identifier: desc
"#,
    );

    assert!(matches!(
        err,
        ValidationError::SatayIdentifierRequiresObjectProperty { context }
            if context == "schema `Description`"
    ));
}

#[test]
fn rejects_explicit_property_identifier_collisions_after_rust_normalization() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Record:
      type: object
      properties:
        Description:
          type: string
          x-satay:
            identifier: request-id
        request_id:
          type: string
"#,
    );

    assert!(matches!(
        err,
        ValidationError::DuplicateSatayIdentifierRustField {
            context,
            first_property,
            second_property,
            rust_name,
        } if context == "schema `Record`"
            && first_property == "Description"
            && second_property == "request_id"
            && rust_name == "request_id"
    ));
}

#[test]
fn rejects_property_identifier_collisions_across_all_of_branches() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Base:
      type: object
      properties:
        Description:
          type: string
          x-satay:
            identifier: request-id
    Record:
      allOf:
        - $ref: '#/components/schemas/Base'
        - type: object
          properties:
            request_id:
              type: string
"#,
    );

    assert!(matches!(
        err,
        ValidationError::DuplicateSatayIdentifierRustField {
            context,
            first_property,
            second_property,
            rust_name,
        } if context == "schema `Record`"
            && first_property == "Description"
            && second_property == "request_id"
            && rust_name == "request_id"
    ));
}

#[test]
fn preserves_legacy_field_deduplication_without_identifier_overrides() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Record:
      type: object
      properties:
        request-id:
          type: string
        request_id:
          type: string
"#,
    );

    let record = component(&api, "Record");
    let ComponentKind::Struct(fields) = &record.kind else {
        panic!("expected Record struct");
    };
    assert_eq!(field(fields, "request-id").rust_name, "request_id");
    assert_eq!(field(fields, "request_id").rust_name, "request_id_2");
}

#[test]
fn rejects_unsupported_x_satay_reference_sibling() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Identifier:
      type: string
    Record:
      type: object
      properties:
        id:
          $ref: '#/components/schemas/Identifier'
          x-satay:
            parse-as: u32
"#,
    );

    match err {
        ValidationError::UnsupportedRefSiblingKeyword { context, keyword } => {
            assert_eq!(context, "property `Record.id`");
            assert_eq!(keyword, "x-satay.parse-as");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn parses_x_satay_none_if_for_parsed_string_fields() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Reading:
      type: object
      required: [wbgt]
      properties:
        wbgt:
          type: string
          x-satay:
            parse-as: f64
            none-if: [NA, "-"]
        optionalWbgt:
          type: string
          x-satay:
            parse-as: f64
            none-if: [NA]
"#,
    );

    let reading = component(&api, "Reading");
    let ComponentKind::Struct(fields) = &reading.kind else {
        panic!("expected Reading struct");
    };
    assert_eq!(field(fields, "wbgt").none_if, ["NA", "-"]);
    assert_eq!(field(fields, "optionalWbgt").none_if, ["NA"]);
}

#[test]
fn parses_configured_boolean_string_mappings() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    TaxiStand:
      type: object
      required: [Bfa]
      properties:
        Bfa:
          type: string
          x-satay:
            parse-as: bool
            true-values: [Y, Yes, "1", "true"]
            false-values: [N, No, "0", "false", ""]
            unknown-as: false
"#,
    );

    let taxi_stand = component(&api, "TaxiStand");
    let ComponentKind::Struct(fields) = &taxi_stand.kind else {
        panic!("expected TaxiStand struct");
    };
    let TypeRef::ParsedString(StringCodec::MappedBool(mapping)) = &field(fields, "Bfa").ty else {
        panic!("expected mapped boolean string codec");
    };
    assert_eq!(mapping.true_values(), ["Y", "Yes", "1", "true"]);
    assert_eq!(mapping.false_values(), ["N", "No", "0", "false", ""]);
    assert_eq!(mapping.unknown_as(), Some(false));
}

#[test]
fn rejects_overlapping_boolean_string_mappings() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    TaxiStand:
      type: object
      properties:
        Bfa:
          type: string
          x-satay:
            parse-as: bool
            true-values: [Y, unknown]
            false-values: [N, unknown]
"#,
    );

    assert!(matches!(
        err,
        ValidationError::OverlappingSatayBoolMapping { context, value }
            if context == "property `TaxiStand.Bfa`" && value == "unknown"
    ));
}

#[test]
fn rejects_invalid_x_satay_none_if_configurations() {
    let cases = [
        ("none-if: []", None),
        ("none-if: NA", Some("x-satay.none-if")),
        ("none-if: [NA, 1]", Some("x-satay.none-if[1]")),
    ];

    for (none_if, invalid_path) in cases {
        let spec = format!(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {{}}
components:
  schemas:
    Reading:
      type: object
      properties:
        wbgt:
          type: string
          x-satay:
            parse-as: f64
            {none_if}
"#
        );
        match (parse_invalid(&spec), invalid_path) {
            (ValidationError::EmptySatayNoneIf { context }, None) => {
                assert_eq!(context, "property `Reading.wbgt`");
            }
            (ValidationError::InvalidExtension { context, path, .. }, Some(expected_path)) => {
                assert_eq!(context, "property `Reading.wbgt`");
                assert_eq!(path, expected_path);
            }
            (other, _) => panic!("unexpected error: {other}"),
        }
    }
}

#[test]
fn rejects_x_satay_none_if_without_string_parser_or_with_lossy_mode() {
    let without_parser = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Reading:
      type: object
      properties:
        wbgt:
          type: string
          x-satay:
            none-if: [NA]
"#,
    );
    assert!(matches!(
        without_parser,
        ValidationError::SatayNoneIfRequiresParsedString { context }
            if context == "property `Reading.wbgt`"
    ));

    let conflicting = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Reading:
      type: object
      properties:
        wbgt:
          type: string
          x-satay:
            parse-as: f64
            none-if: [NA]
            treat-error-as-none: true
"#,
    );
    assert!(matches!(
        conflicting,
        ValidationError::ConflictingSatayNoneHandling { context }
            if context == "property `Reading.wbgt`"
    ));
}

#[test]
fn rejects_x_satay_none_if_outside_supported_parsed_struct_fields() {
    let integer_bool = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Reading:
      type: object
      properties:
        monitored:
          type: integer
          x-satay:
            parse-as: bool
            none-if: [NA]
"#,
    );
    assert!(matches!(
        integer_bool,
        ValidationError::SatayNoneIfRequiresParsedString { context }
            if context == "property `Reading.monitored`"
    ));

    let range = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Reading:
      type: object
      properties:
        range:
          type: string
          x-satay:
            parse-as: number-range
            none-if: [NA]
"#,
    );
    assert!(matches!(
        range,
        ValidationError::SatayNoneIfRequiresParsedString { context }
            if context == "property `Reading.range`"
    ));

    let component = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Wbgt:
      type: string
      x-satay:
        parse-as: f64
        none-if: [NA]
"#,
    );
    assert!(matches!(
        component,
        ValidationError::SatayNoneIfRequiresStructField { context }
            if context == "schema `Wbgt`"
    ));
}

#[test]
fn rejects_x_satay_none_if_on_nullable_parameter_union_wrapper() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /reading:
    get:
      operationId: getReading
      parameters:
        - name: wbgt
          in: query
          schema:
            oneOf:
              - type: string
              - type: "null"
            x-satay:
              parse-as: f64
              none-if: [NA]
      responses:
        '204':
          description: No content
"#,
    );

    match err {
        ValidationError::UnsupportedOneOfSiblingKeyword { context, keyword } => {
            assert_eq!(context, "parameter `wbgt`");
            assert_eq!(keyword, "x-satay");
        }
        other => panic!("unexpected error: {other}"),
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
fn rejects_x_satay_parse_as_bool_with_integer_type_on_property() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Record:
      type: object
      properties:
        id:
          type: integer
          x-satay:
            parse-as: bool
            integer-type: u8
"#,
    );

    match err {
        ValidationError::SatayParseAsBoolWithIntegerType {
            context,
            integer_type,
        } => {
            assert_eq!(context, "property `Record.id`");
            assert_eq!(integer_type, "u8");
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
        ValidationError::InvalidExtension {
            context,
            path,
            source: _,
        } => {
            assert_eq!(context, "property `Arrival.timing`");
            assert_eq!(path, "x-satay.treat-error-as-none");
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

#[test]
fn skips_operations_annotated_with_x_satay_skip() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /files:
    post:
      operationId: uploadFile
      x-satay:
        skip: true
      requestBody:
        required: true
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                file:
                  type: string
      responses:
        '204':
          description: No content
    get:
      operationId: listFiles
      responses:
        '204':
          description: No content
"#,
    );

    assert_eq!(api.operations.len(), 1);
    assert_eq!(api.operations[0].fn_name, "list_files");
}

#[test]
fn validates_operations_with_x_satay_skip_false() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      x-satay:
        skip: false
      responses:
        '204':
          description: No content
"#,
    );

    assert_eq!(api.operations.len(), 1);
    assert_eq!(api.operations[0].fn_name, "ping");
}

#[test]
fn rejects_non_boolean_x_satay_skip() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      x-satay:
        skip: "yes"
      responses:
        '204':
          description: No content
"#,
    );

    match err {
        ValidationError::InvalidExtension {
            context,
            path,
            source: _,
        } => {
            assert_eq!(context, "operation `ping`");
            assert_eq!(path, "x-satay.skip");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_non_object_operation_x_satay() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      x-satay: true
      responses:
        '204':
          description: No content
"#,
    );

    match err {
        ValidationError::InvalidExtension {
            context,
            path,
            source: _,
        } => {
            assert_eq!(context, "operation `ping`");
            assert_eq!(path, "x-satay");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_unknown_operation_x_satay_key() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      x-satay:
        other: true
      responses:
        '204':
          description: No content
"#,
    );

    match err {
        ValidationError::InvalidExtension {
            context,
            path,
            source: _,
        } => {
            assert_eq!(context, "operation `ping`");
            assert_eq!(path, "x-satay.other");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn projects_operation_response_payload_types() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /services:
    get:
      operationId: getServices
      x-satay:
        output:
          unwrap-field: value
      responses:
        '200':
          description: Services
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ServiceEnvelope'
  /links:
    get:
      operationId: getLinks
      x-satay:
        output:
          unwrap-field: value
          map-field: Link
      responses:
        '200':
          description: Links
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/LinkEnvelope'
components:
  schemas:
    ServiceEnvelope:
      type: object
      required: [value]
      properties:
        odata.metadata:
          type: string
        value:
          type: array
          items:
            $ref: '#/components/schemas/Service'
    Service:
      type: object
      required: [id]
      properties:
        id:
          type: string
    LinkEnvelope:
      type: object
      required: [value]
      properties:
        value:
          type: array
          items:
            $ref: '#/components/schemas/LinkRow'
    LinkRow:
      type: object
      required: [Link]
      properties:
        Link:
          type: string
"#,
    );

    let services = api
        .operations
        .iter()
        .find(|operation| operation.fn_name == "get_services")
        .expect("services operation");
    let response = &services.responses[0];
    assert_eq!(
        response.body,
        Some(TypeRef::Array(Box::new(TypeRef::Named(
            "Service".to_owned()
        ))))
    );
    let projection = response.projection.as_ref().expect("projection");
    assert_eq!(projection.unwrap_field, "value");
    assert_eq!(projection.map_field, None);

    let links = api
        .operations
        .iter()
        .find(|operation| operation.fn_name == "get_links")
        .expect("links operation");
    let response = &links.responses[0];
    assert_eq!(
        response.body,
        Some(TypeRef::Array(Box::new(TypeRef::String)))
    );
    let projection = response.projection.as_ref().expect("projection");
    assert_eq!(projection.unwrap_field, "value");
    assert_eq!(projection.map_field.as_deref(), Some("Link"));
}

#[test]
fn rejects_unknown_x_satay_output_fields() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /services:
    get:
      operationId: getServices
      x-satay:
        output:
          unwrap-field: missing
      responses:
        '200':
          description: Services
          content:
            application/json:
              schema:
                type: object
                properties:
                  value:
                    type: array
                    items:
                      type: string
"#,
    );

    assert!(matches!(
        err,
        ValidationError::UnknownSatayOutputField {
            context,
            selector: "unwrap-field",
            field,
        } if context == "operation `getServices` responses 200 schema" && field == "missing"
    ));
}

#[test]
fn rejects_x_satay_output_map_field_for_non_array_payload() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /link:
    get:
      operationId: getLink
      x-satay:
        output:
          unwrap-field: value
          map-field: Link
      responses:
        '200':
          description: Link
          content:
            application/json:
              schema:
                type: object
                required: [value]
                properties:
                  value:
                    type: string
"#,
    );

    assert!(matches!(
        err,
        ValidationError::SatayOutputMapRequiresArray { context, field }
            if context == "operation `getLink` responses 200 schema" && field == "value"
    ));
}

#[test]
fn rejects_x_satay_output_without_a_json_response_body() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /health:
    get:
      operationId: health
      x-satay:
        output:
          unwrap-field: value
      responses:
        '204':
          description: No content
"#,
    );

    assert!(matches!(
        err,
        ValidationError::SatayOutputRequiresResponseBody { operation_id }
            if operation_id == "health"
    ));
}

#[test]
fn skips_component_schema_used_only_by_skipped_operation() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /files:
    post:
      operationId: uploadFile
      x-satay:
        skip: true
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/UploadRequest'
      responses:
        '204':
          description: No content
    get:
      operationId: listFiles
      responses:
        '204':
          description: No content
components:
  schemas:
    UploadRequest:
      type: object
      required:
        - flag
      properties:
        flag:
          type: boolean
          x-satay:
            parse-as: u8
"#,
    );

    assert_eq!(api.operations.len(), 1);
    assert_eq!(api.operations[0].fn_name, "list_files");
    assert!(
        api.components
            .iter()
            .all(|component| component.rust_name != "UploadRequest"),
        "skipped-only component must be excluded from generation"
    );
}

#[test]
fn skips_component_schema_used_by_skipped_content_parameter() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /files:
    get:
      operationId: searchFiles
      x-satay:
        skip: true
      parameters:
        - name: filter
          in: query
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BrokenFilter'
      responses:
        '204':
          description: No content
  /health:
    get:
      operationId: health
      responses:
        '204':
          description: No content
components:
  schemas:
    BrokenFilter:
      type: boolean
      x-satay:
        parse-as: u8
"#,
    );

    assert_eq!(api.operations.len(), 1);
    assert_eq!(api.operations[0].fn_name, "health");
    assert!(
        api.components
            .iter()
            .all(|component| component.rust_name != "BrokenFilter"),
        "component used only by a skipped content parameter must be excluded"
    );
}

#[test]
fn skips_component_schema_reached_through_prefix_items() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /files:
    post:
      operationId: uploadFiles
      x-satay:
        skip: true
      requestBody:
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/UploadTuple'
      responses:
        '204':
          description: No content
  /health:
    get:
      operationId: health
      responses:
        '204':
          description: No content
components:
  schemas:
    UploadTuple:
      type: array
      prefixItems:
        - $ref: '#/components/schemas/BrokenItem'
    BrokenItem:
      type: boolean
      x-satay:
        parse-as: u8
"#,
    );

    assert_eq!(api.operations.len(), 1);
    assert_eq!(api.operations[0].fn_name, "health");
    assert!(
        api.components.iter().all(|component| {
            !matches!(component.rust_name.as_str(), "UploadTuple" | "BrokenItem")
        }),
        "the complete skipped-only prefixItems graph must be excluded"
    );
}

#[test]
fn validates_component_schema_shared_with_non_skipped_operation() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /files:
    post:
      operationId: uploadFile
      x-satay:
        skip: true
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/Shared'
      responses:
        '204':
          description: No content
    get:
      operationId: getShared
      responses:
        '200':
          description: OK
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Shared'
components:
  schemas:
    Shared:
      type: object
      required:
        - id
      properties:
        id:
          type: string
"#,
    );

    assert_eq!(api.operations.len(), 1);
    component(&api, "Shared");
}

#[test]
fn keeps_unreferenced_component_schema_when_operation_is_skipped() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /files:
    post:
      operationId: uploadFile
      x-satay:
        skip: true
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/A'
      responses:
        '204':
          description: No content
    get:
      operationId: listFiles
      responses:
        '204':
          description: No content
components:
  schemas:
    A:
      type: object
      required:
        - flag
      properties:
        flag:
          type: boolean
          x-satay:
            parse-as: u8
    Orphan:
      type: object
      required:
        - value
      properties:
        value:
          type: string
"#,
    );

    assert_eq!(api.operations.len(), 1);
    component(&api, "Orphan");
    assert!(
        api.components
            .iter()
            .all(|component| component.rust_name != "A"),
        "skipped-only rejectable component must be excluded"
    );
}

#[test]
fn keeps_skipped_only_schema_referenced_by_unreferenced_component() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /files:
    post:
      operationId: uploadFile
      x-satay:
        skip: true
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/Shared'
      responses:
        '204':
          description: No content
    get:
      operationId: listFiles
      responses:
        '204':
          description: No content
components:
  schemas:
    Shared:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Holder:
      type: object
      required:
        - x
      properties:
        x:
          $ref: '#/components/schemas/Shared'
"#,
    );

    assert_eq!(api.operations.len(), 1);
    component(&api, "Shared");
    component(&api, "Holder");
}

#[test]
fn skips_path_level_parameters_when_all_operations_on_path_skipped() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /files/{id}:
    parameters:
      - name: id
        in: path
        required: true
        schema:
          type: array
          items:
            type: string
    delete:
      operationId: deleteFile
      x-satay:
        skip: true
      responses:
        '204':
          description: No content
  /health:
    get:
      operationId: health
      responses:
        '204':
          description: No content
"#,
    );

    assert_eq!(api.operations.len(), 1);
    assert_eq!(api.operations[0].fn_name, "health");
}

#[test]
fn unchanged_component_validation_without_skip() {
    let api = parse_valid(
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
  schemas:
    Unused:
      type: object
      required:
        - value
      properties:
        value:
          type: string
"#,
    );

    assert_eq!(api.operations.len(), 1);
    component(&api, "Unused");
}
