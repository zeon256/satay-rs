use super::ast::*;
use super::*;
use satay_ir::{PropertyPolicy, StringInterpretation, StringScalar, TypeExpr};
use syn::{Fields, ImplItem, Item};

/// Extracts the method names of the untagged `Api` view, in declaration order.
fn untagged_methods(file: &syn::File) -> Vec<String> {
    file.items
        .iter()
        .find_map(|item| {
            let Item::Impl(item_impl) = item else {
                return None;
            };
            (norm(&item_impl.self_ty).contains("Api")).then(|| {
                item_impl
                    .items
                    .iter()
                    .filter_map(|impl_item| match impl_item {
                        ImplItem::Fn(method) if is_pub(&method.vis) => {
                            Some(method.sig.ident.to_string())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_default()
}

/// Extracts the payload type of the `Ok` response variant.
fn ok_payload(file: &syn::File, response_name: &str) -> String {
    let response = find_enum(file, response_name);
    let ok = variant(response, "Ok");
    let Fields::Unnamed(fields) = &ok.fields else {
        panic!("`{response_name}::Ok` must be a tuple variant");
    };
    norm(&fields.unnamed[0].ty)
}

/// Looks up one object property's inclusion policy in the semantic IR.
fn property_policy<'a>(
    api: &'a satay_ir::Api,
    definition_name: &str,
    wire_name: &str,
) -> &'a PropertyPolicy {
    let definition = api
        .definitions()
        .find_map(|(_, definition)| {
            (definition.source_name == definition_name).then_some(definition)
        })
        .unwrap_or_else(|| panic!("missing definition `{definition_name}`"));
    let TypeExpr::Object(object) = &definition.schema.ty else {
        panic!("definition `{definition_name}` must be an object schema");
    };
    let property = object
        .properties
        .iter()
        .find(|property| property.wire_name == wire_name)
        .unwrap_or_else(|| panic!("missing property `{definition_name}.{wire_name}`"));
    &property.policy
}

/// Looks up one parameter's string interpretation in the semantic IR.
fn parameter_interpretation<'a>(
    api: &'a satay_ir::Api,
    operation_id: &str,
    wire_name: &str,
) -> &'a StringInterpretation {
    let operation = api
        .http()
        .paths
        .iter()
        .flat_map(|path| &path.operations)
        .find(|operation| operation.source_id.as_deref() == Some(operation_id))
        .unwrap_or_else(|| panic!("missing operation `{operation_id}`"));
    let parameter = operation
        .parameters
        .iter()
        .find(|parameter| parameter.wire_name == wire_name)
        .unwrap_or_else(|| panic!("missing parameter `{wire_name}`"));
    let TypeExpr::String(string) = &parameter.schema.ty else {
        panic!("parameter `{wire_name}` must be a string schema");
    };
    &string.interpretation
}

#[test]
fn parses_x_satay_parse_as_for_string_schemas() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let arrival = find_struct(&types, "Arrival");

    // String wire schemas lower to parsed scalars; the serde codec names the
    // requested parse-as.
    assert_field(arrival, "stop", "u32");
    assert_attr_contains(
        &field(arrival, "stop").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_u32""#,
    );
    assert_field(arrival, "latitude", "f64");
    assert_attr_contains(
        &field(arrival, "latitude").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_f64""#,
    );
    assert_field(arrival, "visit", "u8");
    assert_attr_contains(
        &field(arrival, "visit").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_u8""#,
    );
    assert_field(arrival, "monitored", "bool");
    assert_attr_contains(
        &field(arrival, "monitored").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_bool""#,
    );
    // Integer wire schemas parse as bool through the integer codec.
    assert_field(arrival, "numeric_monitored", "bool");
    assert_attr_contains(
        &field(arrival, "numeric_monitored").attrs,
        "cfg_attr",
        r#"with = "serde_integer::as_bool""#,
    );
    assert_field(
        arrival,
        "estimated_arrival",
        "satay_runtime::OffsetDateTime",
    );
    assert_attr_contains(
        &field(arrival, "estimated_arrival").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_offset_datetime""#,
    );

    // Constrained strings lower to range components over the declared scalar.
    assert_field(arrival, "frequency", "ArrivalFrequency");
    let frequency = find_struct(&types, "ArrivalFrequency");
    assert_field(frequency, "min", "Option<u8>");
    assert_field(frequency, "max", "Option<u8>");
    assert!(contains_tokens(&types, "satay_runtime::parse_range::<u8>"));
    assert_field(arrival, "ratio", "ArrivalRatio");
    let ratio = find_struct(&types, "ArrivalRatio");
    assert_field(ratio, "min", "Option<f32>");
    assert_field(ratio, "max", "Option<f32>");
    assert!(contains_tokens(&types, "satay_runtime::parse_range::<f32>"));
}

#[test]
fn lowers_date_parse_as_on_query_parameters() {
    let spec = r#"
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
"#;

    let api = normalize_spec(spec);
    assert_eq!(
        parameter_interpretation(&api, "psi", "date"),
        &StringInterpretation::Scalar(StringScalar::Date),
    );

    let files = generate_valid(spec);
    let parts = parse_rust(file(&files, "psi/parts.rs"));
    let input = find_struct(&parts, "PsiInput");
    // The optional query parameter wraps the parsed date in `Option`.
    assert_field(input, "date", "Option<satay_runtime::Date>");
    assert!(has_method(&parts, "PsiInput", "date"));
    let parts_fn = find_fn(&parts, "psi_parts");
    assert!(contains_tokens(
        parts_fn,
        "satay_runtime::format_date(value)"
    ));
}

#[test]
fn lowers_naive_datetime_parse_as_on_query_parameters() {
    let spec = r#"
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
"#;

    let api = normalize_spec(spec);
    assert_eq!(
        parameter_interpretation(&api, "psi", "date"),
        &StringInterpretation::Scalar(StringScalar::NaiveDatetime),
    );

    let files = generate_valid(spec);
    let parts = parse_rust(file(&files, "psi/parts.rs"));
    let input = find_struct(&parts, "PsiInput");
    // The optional query parameter wraps the parsed datetime in `Option`.
    assert_field(input, "date", "Option<satay_runtime::PrimitiveDateTime>");
    assert!(has_method(&parts, "PsiInput", "date"));
    let parts_fn = find_fn(&parts, "psi_parts");
    assert!(contains_tokens(
        parts_fn,
        "satay_runtime::format_naive_datetime(value)"
    ));
}

#[test]
fn parses_x_satay_enum_variants() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));

    // NOTE: `EnumFallback::None` is pinned by the exact variant list — no
    // extra fallback variant is appended beyond the four declared names.
    let vehicle_type = find_enum(&types, "VehicleType");
    assert_eq!(
        variant_names(vehicle_type),
        ["SingleDecker", "DoubleDecker", "Bendy", "Unknown"]
    );
    // Wire names survive as serde renames.
    assert_attr_contains(
        &variant(vehicle_type, "SingleDecker").attrs,
        "cfg_attr",
        r#"serde(rename = "SD")"#,
    );
    assert_attr_contains(
        &variant(vehicle_type, "DoubleDecker").attrs,
        "cfg_attr",
        r#"serde(rename = "DD")"#,
    );
    assert_attr_contains(
        &variant(vehicle_type, "Bendy").attrs,
        "cfg_attr",
        r#"serde(rename = "BD")"#,
    );
    assert_attr_contains(
        &variant(vehicle_type, "Unknown").attrs,
        "cfg_attr",
        r#"serde(rename = "")"#,
    );

    let arrival = find_struct(&types, "Arrival");
    assert_field(arrival, "r#type", "ArrivalType");
    let arrival_type = find_enum(&types, "ArrivalType");
    assert_eq!(
        variant_names(arrival_type),
        ["SingleDecker", "DoubleDecker", "Bendy", "Unknown"]
    );
    assert_attr_contains(
        &variant(arrival_type, "SingleDecker").attrs,
        "cfg_attr",
        r#"serde(rename = "SD")"#,
    );
    assert_attr_contains(
        &variant(arrival_type, "Unknown").attrs,
        "cfg_attr",
        r#"serde(rename = "")"#,
    );
}

#[test]
fn parses_x_satay_enum_variants_using_other_for_closed_enum() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let vehicle_type = find_enum(&types, "VehicleType");
    assert_eq!(variant_names(vehicle_type), ["Other"]);
    assert_attr_contains(
        &variant(vehicle_type, "Other").attrs,
        "cfg_attr",
        r#"serde(rename = "SD")"#,
    );
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
fn rejects_inapplicable_x_satay_options_on_component_enums() {
    for (option, expected_keyword) in [
        ("integer-type: auto", "integer-type"),
        ("true-values: [Y]", "true-values"),
        ("false-values: [N]", "false-values"),
        ("unknown-as: false", "unknown-as"),
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
    Status:
      type: string
      enum: [ready]
      x-satay:
        {option}
"#
        ));

        assert!(matches!(
            err,
            ValidationError::SatayOptionUnsupportedWithEnum { context, keyword }
                if context == "schema `Status`" && keyword == expected_keyword
        ));
    }
}

#[test]
fn rejects_inapplicable_x_satay_options_on_ignored_enum_properties() {
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
        status:
          type: integer
          enum: [1]
          x-satay:
            ignore: true
            integer-type: auto
"#,
    );

    assert!(matches!(
        err,
        ValidationError::SatayOptionUnsupportedWithEnum {
            context,
            keyword: "integer-type",
        } if context == "property `Record.status`"
    ));
}

#[test]
fn rejects_x_satay_type_options_on_component_structs() {
    let spec = |option: &str| {
        format!(
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
        value:
          type: string
      x-satay:
        {option}
"#
        )
    };

    assert!(matches!(
        parse_invalid(&spec("parse-as: u8")),
        ValidationError::SatayParseAsRequiresString {
            context,
            parse_as,
            kind,
        } if context == "schema `Record`" && parse_as == "u8" && kind == "object"
    ));
    assert!(matches!(
        parse_invalid(&spec("integer-type: auto")),
        ValidationError::SatayIntegerTypeRequiresInteger {
            context,
            integer_type,
            kind,
        } if context == "schema `Record`" && integer_type == "auto" && kind == "object"
    ));
    assert!(matches!(
        parse_invalid(&spec("true-values: [Y]")),
        ValidationError::SatayBoolMappingRequiresParsedStringBool { context }
            if context == "schema `Record`"
    ));
}

#[test]
fn rejects_all_type_options_on_property_ref_siblings() {
    for (option, expected_keyword) in [
        ("parse-as: u8", "parse-as"),
        ("integer-type: auto", "integer-type"),
        ("none-if: []", "none-if"),
        ("true-values: []", "true-values"),
        ("false-values: []", "false-values"),
        ("unknown-as: false", "unknown-as"),
        ("enum-variants: {}", "enum-variants"),
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
    Value:
      type: string
    Record:
      type: object
      properties:
        value:
          $ref: '#/components/schemas/Value'
          x-satay:
            ignore: true
            {option}
"#
        ));

        assert!(matches!(
            err,
            ValidationError::UnsupportedRefSiblingKeyword { context, keyword }
                if context == "property `Record.value`"
                    && keyword == format!("x-satay.{expected_keyword}")
        ));
    }
}

#[test]
fn rejects_property_options_on_value_ref_siblings_by_keyword() {
    for (option, expected_keyword) in [
        ("treat-error-as-none: false", "treat-error-as-none"),
        ("ignore: false", "ignore"),
        ("identifier: value", "identifier"),
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
    Value:
      type: string
    Alias:
      $ref: '#/components/schemas/Value'
      x-satay:
        {option}
"#
        ));

        assert!(matches!(
            err,
            ValidationError::UnsupportedRefSiblingKeyword { context, keyword }
                if context == "schema `Alias`"
                    && keyword == format!("x-satay.{expected_keyword}")
        ));
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
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let arrival = find_struct(&types, "Arrival");

    // NOTE: `treat_error_as_none` is expressed by `Option` wrapping plus the
    // runtime serde helpers on the generated field.
    let timing = field(arrival, "timing");
    assert_eq!(norm(&timing.ty), norm_str("Option<S>"));
    assert_attr_contains(
        &timing.attrs,
        "cfg_attr",
        r#""treat_error_as_none::deserialize""#,
    );
    assert_attr_contains(
        &timing.attrs,
        "cfg_attr",
        r#""treat_error_as_none::serialize""#,
    );
    let optional_timing = field(arrival, "optional_timing");
    assert_eq!(norm(&optional_timing.ty), norm_str("Option<S>"));
    assert!(
        optional_timing
            .attrs
            .iter()
            .all(|attr| !norm(attr).contains(&norm_str("treat_error_as_none")))
    );

    // The operation decode body keeps decoding the whole struct.
    let json = parse_rust(file(&files, "get_arrival/json.rs"));
    let decode = find_fn(&json, "decode_get_arrival_response");
    assert!(contains_tokens(decode, "satay_runtime::from_json_slice"));
    let parts = parse_rust(file(&files, "get_arrival/parts.rs"));
    assert_eq!(
        ok_payload(&parts, "GetArrivalResponse"),
        norm_str("Arrival<S>")
    );
}

#[test]
fn parses_x_satay_treat_error_as_none_on_required_reference_property() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let arrival = find_struct(&types, "BusServiceArrival");

    // NOTE: `required` plus `treat-error-as-none` lowers to `Option` with the
    // runtime serde helpers; without the extension the required reference
    // stays a direct value.
    let next_bus = field(arrival, "next_bus");
    assert_eq!(norm(&next_bus.ty), norm_str("Option<BusArrivalTiming<S>>"));
    assert_attr_contains(
        &next_bus.attrs,
        "cfg_attr",
        r#""treat_error_as_none::deserialize""#,
    );
    let strict_next_bus = field(arrival, "strict_next_bus");
    assert_eq!(norm(&strict_next_bus.ty), norm_str("BusArrivalTiming<S>"));
    assert!(
        strict_next_bus
            .attrs
            .iter()
            .all(|attr| !norm(attr).contains(&norm_str("treat_error_as_none")))
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
fn rejects_x_satay_ignore_true_with_inline_property_options() {
    for (property_schema, expected_keyword) in [
        (
            r#"          type: string
          x-satay:
            ignore: true
            identifier: value"#,
            "identifier",
        ),
        (
            r#"          type: string
          x-satay:
            ignore: true
            treat-error-as-none: true"#,
            "treat-error-as-none",
        ),
        (
            r#"          type: string
          x-satay:
            ignore: true
            parse-as: u32"#,
            "parse-as",
        ),
        (
            r#"          type: string
          x-satay:
            ignore: true
            parse-as: u32
            none-if: ['']"#,
            "none-if",
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
    Record:
      type: object
      properties:
        value:
{property_schema}
"#
        ));
        let message = err.to_string();

        match err {
            ValidationError::SatayOptionConflictsWithIgnore { context, keyword } => {
                assert_eq!(context, "property `Record.value`");
                assert_eq!(keyword, expected_keyword);
            }
            other => panic!("unexpected error: {other}"),
        }
        assert_eq!(
            message,
            format!(
                "property `Record.value` cannot combine x-satay.ignore `true` with x-satay.{expected_keyword}"
            )
        );
    }
}

#[test]
fn rejects_x_satay_ignore_true_with_enum_variants() {
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
        status:
          type: string
          enum: [ready]
          x-satay:
            ignore: true
            enum-variants:
              ready: ReadyState
"#,
    );

    assert!(matches!(
        err,
        ValidationError::SatayOptionConflictsWithIgnore {
            context,
            keyword: "enum-variants",
        } if context == "property `Record.status`"
    ));
}

#[test]
fn rejects_x_satay_ignore_true_with_property_ref_option_presence() {
    let err = parse_invalid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Value:
      type: string
    Record:
      type: object
      properties:
        value:
          $ref: '#/components/schemas/Value'
          x-satay:
            ignore: true
            treat-error-as-none: false
"#,
    );

    assert!(matches!(
        err,
        ValidationError::SatayOptionConflictsWithIgnore {
            context,
            keyword: "treat-error-as-none",
        } if context == "property `Record.value`"
    ));
}

#[test]
fn omits_x_satay_ignored_object_properties() {
    let files = generate_valid(
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
            treat-error-as-none: false
            identifier: kept
        BusStopCode:
          type: string
"#,
    );

    let types = parse_rust(file(&files, "types.rs"));
    let response = find_struct(&types, "BusArrivalResponse");

    // Ignored properties disappear entirely; only the retained ones remain.
    assert_eq!(field_names(response), ["kept", "bus_stop_code"]);
    // NOTE: `identifier: kept` renames the retained field; plain semantics
    // (no treat-error-as-none) keep it a bare optional value.
    let retained = field(response, "kept");
    assert_eq!(norm(&retained.ty), norm_str("Option<S>"));
    assert_attr_contains(
        &retained.attrs,
        "cfg_attr",
        r#"rename = "retainedMetadata""#,
    );
    assert!(
        retained
            .attrs
            .iter()
            .all(|attr| !norm(attr).contains(&norm_str("treat_error_as_none")))
    );
    assert_field(response, "bus_stop_code", "S");
    assert_attr_contains(
        &field(response, "bus_stop_code").attrs,
        "cfg_attr",
        r#"serde(rename = "BusStopCode")"#,
    );
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
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let record = find_struct(&types, "Record");
    // Only the identified property remains, claiming the ignored wire name.
    assert_eq!(field_names(record), ["value"]);
    assert_attr_contains(
        &field(record, "value").attrs,
        "cfg_attr",
        r#"rename = "display""#,
    );
}

#[test]
fn parses_target_neutral_property_identifiers_into_ir() {
    let spec = r#"
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
"#;

    // NOTE: the target-neutral identifier words stay in the IR; the private
    // model's `rust_name` facts lower to the generated field names below.
    let api = normalize_spec(spec);
    let PropertyPolicy::Included { identifier, .. } =
        property_policy(&api, "BusStop", "Description")
    else {
        panic!("Description must participate in decoding");
    };
    assert_eq!(identifier.as_deref(), Some(["desc".to_owned()].as_slice()));
    let PropertyPolicy::Included { identifier, .. } =
        property_policy(&api, "BusStop", "RequestIdentifier")
    else {
        panic!("RequestIdentifier must participate in decoding");
    };
    assert_eq!(
        identifier.as_deref(),
        Some(["request".to_owned(), "id".to_owned()].as_slice())
    );
    let PropertyPolicy::Included { identifier, .. } = property_policy(&api, "BusStop", "RoadName")
    else {
        panic!("RoadName must participate in decoding");
    };
    assert!(identifier.is_none());

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));
    let bus_stop = find_struct(&types, "BusStop");
    assert_eq!(
        field_names(bus_stop),
        ["desc", "request_id", "reference_id", "road_name", "r#type"]
    );
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
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let record = find_struct(&types, "Record");
    assert_eq!(field_names(record), ["request_id", "request_id_2"]);
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
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let reading = find_struct(&types, "Reading");

    // NOTE: `none_if` sentinel lists lower to codec-aware `Option` wrapping
    // plus the generated serde helpers on the field.
    assert_field(reading, "wbgt", "Option<f64>");
    let deserialize = find_method(&types, "Reading", "__satay_deserialize_wbgt_none_if");
    assert!(contains_tokens(
        deserialize,
        r#"as_f64::deserialize_none_if(deserializer, &["NA", "-"])"#
    ));
    assert_field(reading, "optional_wbgt", "Option<f64>");
    let deserialize_optional = find_method(
        &types,
        "Reading",
        "__satay_deserialize_optional_wbgt_none_if",
    );
    assert!(contains_tokens(
        deserialize_optional,
        r#"as_f64_option::deserialize_none_if(deserializer, &["NA"])"#
    ));
}

#[test]
fn parses_configured_boolean_string_mappings() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let taxi_stand = find_struct(&types, "TaxiStand");
    assert_field(taxi_stand, "bfa", "bool");
    // NOTE: the mapped bool codec lowers to the generated serde helper
    // carrying the true/false wire lists and the unknown-as fallback.
    let deserialize = find_method(&types, "TaxiStand", "__satay_deserialize_bfa_bool_mapping");
    assert!(contains_tokens(deserialize, "as_bool::deserialize_mapped"));
    assert!(contains_tokens(
        deserialize,
        r#"deserializer, &["Y", "Yes", "1", "true"], &["N", "No", "0", "false", ""], Some(false)"#
    ));
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
    generate_valid(
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
    let spec = r#"
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
"#;
    let files = generate_valid(spec);
    ir::assert_selection(spec, &[], &["listFiles"]);

    // NOTE: the private model's `fn_name` facts surface as the untagged view
    // methods of the generated API.
    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(untagged_methods(&untagged), ["list_files"]);
}

#[test]
fn validates_operations_with_x_satay_skip_false() {
    let files = generate_valid(
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

    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(untagged_methods(&untagged), ["ping"]);
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
    let files = generate_valid(
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

    // NOTE: the private model's `response.body` and `projection` facts surface
    // as the projected decode body and the `Ok` payload type of the generated
    // response enum.
    // getServices: the envelope unwraps `value` into `Vec<Service<S>>`.
    let services_parts = parse_rust(file(&files, "get_services/parts.rs"));
    assert_eq!(
        ok_payload(&services_parts, "GetServicesResponse"),
        norm_str("Vec<Service<S>>")
    );
    let services_json = parse_rust(file(&files, "get_services/json.rs"));
    let decode = find_fn(&services_json, "decode_get_services_response");
    assert!(contains_tokens(
        decode,
        "satay_runtime::from_projected_json_slice"
    ));
    assert!(contains_tokens(decode, r#"(body, "value", None)"#));

    // getLinks: mapping `Link` further projects the payload to `Vec<S>`.
    let links_parts = parse_rust(file(&files, "get_links/parts.rs"));
    assert_eq!(
        ok_payload(&links_parts, "GetLinksResponse"),
        norm_str("Vec<S>")
    );
    let links_json = parse_rust(file(&files, "get_links/json.rs"));
    let decode = find_fn(&links_json, "decode_get_links_response");
    assert!(contains_tokens(
        decode,
        "satay_runtime::from_projected_json_slice"
    ));
    assert!(contains_tokens(decode, r#"(body, "value", Some("Link"))"#));
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
    let spec = r#"
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
"#;
    let files = generate_valid(spec);
    ir::assert_selection(spec, &[], &["listFiles"]);

    // NOTE: no component survives, so no `types.rs` is emitted at all; the
    // exclusion is asserted across every generated file.
    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(untagged_methods(&untagged), ["list_files"]);
    assert!(
        files
            .iter()
            .all(|generated| !generated.contents.contains("UploadRequest")),
        "skipped-only component must be excluded from generation"
    );
}

#[test]
fn skips_component_schema_used_by_skipped_content_parameter() {
    let spec = r#"
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
"#;
    let files = generate_valid(spec);
    ir::assert_selection(spec, &[], &["health"]);

    // NOTE: no component survives, so no `types.rs` is emitted at all; the
    // exclusion is asserted across every generated file.
    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(untagged_methods(&untagged), ["health"]);
    assert!(
        files
            .iter()
            .all(|generated| !generated.contents.contains("BrokenFilter")),
        "component used only by a skipped content parameter must be excluded"
    );
}

#[test]
fn skips_component_schema_reached_through_prefix_items() {
    let spec = r#"
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
"#;
    let files = generate_valid(spec);
    ir::assert_selection(spec, &[], &["health"]);

    // NOTE: no component survives, so no `types.rs` is emitted at all; the
    // exclusion is asserted across every generated file.
    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(untagged_methods(&untagged), ["health"]);
    assert!(
        files.iter().all(|generated| {
            !generated.contents.contains("UploadTuple")
                && !generated.contents.contains("BrokenItem")
        }),
        "the complete skipped-only prefixItems graph must be excluded"
    );
}

#[test]
fn validates_component_schema_shared_with_non_skipped_operation() {
    let spec = r#"
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
"#;
    let files = generate_valid(spec);
    ir::assert_selection(spec, &["Shared"], &["getShared"]);

    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(untagged_methods(&untagged), ["get_shared"]);
    let types = parse_rust(file(&files, "types.rs"));
    find_struct(&types, "Shared");
}

#[test]
fn keeps_unreferenced_component_schema_when_operation_is_skipped() {
    let spec = r#"
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
"#;
    let files = generate_valid(spec);
    ir::assert_selection(spec, &["Orphan"], &["listFiles"]);

    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(untagged_methods(&untagged), ["list_files"]);
    let types = parse_rust(file(&files, "types.rs"));
    find_struct(&types, "Orphan");
    assert!(
        !contains_ident(&types, "A"),
        "skipped-only rejectable component must be excluded"
    );
}

#[test]
fn keeps_skipped_only_schema_referenced_by_unreferenced_component() {
    let spec = r#"
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
"#;
    let files = generate_valid(spec);
    ir::assert_selection(spec, &["Shared", "Holder"], &["listFiles"]);

    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(untagged_methods(&untagged), ["list_files"]);
    let types = parse_rust(file(&files, "types.rs"));
    find_struct(&types, "Shared");
    find_struct(&types, "Holder");
}

#[test]
fn skips_path_level_parameters_when_all_operations_on_path_skipped() {
    let spec = r#"
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
"#;

    let files = generate_valid(spec);
    ir::assert_selection(spec, &[], &["health"]);

    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(untagged_methods(&untagged), ["health"]);
}

#[test]
fn unchanged_component_validation_without_skip() {
    let files = generate_valid(
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

    let untagged = parse_rust(file(&files, "untagged.rs"));
    assert_eq!(untagged_methods(&untagged), ["ping"]);
    let types = parse_rust(file(&files, "types.rs"));
    find_struct(&types, "Unused");
}

#[test]
fn uri_format_parses_strings_and_preserves_explicit_overrides() {
    let files = generate_valid(
        r#"
openapi: 3.1.0
info:
  title: URL formats
  version: 1.0.0
paths: {}
components:
  schemas:
    Record:
      type: object
      properties:
        url:
          type: string
          format: uri
        reference:
          type: string
          format: uri-reference
        plain:
          type: string
        override:
          type: string
          format: uri
          x-satay:
            parse-as: u32
        flag:
          type: boolean
          format: uri
"#,
    );

    let types = parse_rust(file(&files, "types.rs"));
    let record = find_struct(&types, "Record");
    // `format: uri` parses the string into the runtime URL type.
    assert_field(record, "url", "Option<satay_runtime::Url>");
    assert_attr_contains(
        &field(record, "url").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_url::option""#,
    );
    // Other string formats stay plain; `uri-reference` is not a URL parse.
    assert_field(record, "reference", "Option<S>");
    assert!(
        field(record, "reference")
            .attrs
            .iter()
            .all(|attr| !norm(attr).contains(&norm_str("as_url")))
    );
    assert_field(record, "plain", "Option<S>");
    // An explicit parse-as overrides the format.
    assert_field(record, "r#override", "Option<u32>");
    assert_attr_contains(
        &field(record, "r#override").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_u32::option""#,
    );
    // Boolean schemas are untouched by string formats.
    assert_field(record, "flag", "Option<bool>");
}

#[test]
fn rejects_string_constraints_on_uri_conversion() {
    for (keyword, value) in [
        ("pattern", "'^https://'"),
        ("minLength", "0"),
        ("maxLength", "32"),
    ] {
        for (schema, context) in [
            (
                format!(
                    "    Link:\n      type: string\n      format: uri\n      {keyword}: {value}\n"
                ),
                "schema `Link`",
            ),
            (
                format!(
                    "    Record:\n      type: object\n      properties:\n        link:\n          type: [string, 'null']\n          format: uri\n          {keyword}: {value}\n"
                ),
                "property `Record.link`",
            ),
            (
                format!(
                    "    Links:\n      type: array\n      items:\n        type: string\n        format: uri\n        {keyword}: {value}\n"
                ),
                "schema `Links` items",
            ),
            (
                format!(
                    "    Links:\n      type: object\n      additionalProperties:\n        type: string\n        format: uri\n        {keyword}: {value}\n"
                ),
                "schema `Links` additionalProperties",
            ),
        ] {
            let err = parse_invalid(&format!(
                "openapi: 3.1.0\ninfo:\n  title: Constrained URLs\n  version: 1.0.0\npaths: {{}}\ncomponents:\n  schemas:\n{schema}"
            ));
            assert!(matches!(
                err,
                ValidationError::UnsupportedKeyword { context: actual_context, keyword: actual_keyword }
                    if actual_context == context && actual_keyword == keyword
            ));
        }
    }
}
