use satay_ir::{
    AdditionalProperties, CompositionKind, DecodePolicy, IntegerInterpretation,
    IntegerRepresentation, NumericBound, NumericConstraints, Property, PropertyPolicy,
    StringInterpretation, StringScalar, TypeExpr,
};
use serde_json::Number;

use super::{definition, normalize, object, string};
use crate::error::ValidationError;
use crate::parse::normalize::{NormalizeError, normalize_spec};

fn property<'a>(properties: &'a [Property], name: &str) -> &'a Property {
    properties
        .iter()
        .find(|property| property.wire_name == name)
        .unwrap()
}

fn validation_at(spec: &str, pointer: &str) -> ValidationError {
    match normalize_spec(spec, "test.yaml").unwrap_err() {
        NormalizeError::Validation { location, source } => {
            assert_eq!(location.document, "test.yaml");
            assert_eq!(location.pointer, pointer);
            *source
        }
        other => panic!("expected a located validation error, got {other:?}"),
    }
}

fn property_spec(schema: &str) -> String {
    format!(
        r#"
openapi: 3.1.0
info: {{title: Interpretation, version: '1'}}
paths: {{}}
components:
  schemas:
    Record:
      type: object
      properties:
        'value/~': {schema}
"#
    )
}

fn property_error(schema: &str) -> ValidationError {
    validation_at(
        &property_spec(schema),
        "/components/schemas/Record/properties/value~1~0/x-satay",
    )
}

#[test]
fn scalar_interpretations_preserve_explicit_wire_types_and_string_constraints() {
    let scalars = [
        ("u8", StringScalar::U8),
        ("u16", StringScalar::U16),
        ("u32", StringScalar::U32),
        ("u64", StringScalar::U64),
        ("i8", StringScalar::I8),
        ("i16", StringScalar::I16),
        ("i32", StringScalar::I32),
        ("i64", StringScalar::I64),
        ("f32", StringScalar::F32),
        ("f64", StringScalar::F64),
        ("bool", StringScalar::Bool),
        ("date", StringScalar::Date),
        ("naive-datetime", StringScalar::NaiveDatetime),
        ("offset-datetime", StringScalar::OffsetDatetime),
        ("time", StringScalar::Time),
    ];
    for (wire, scalar) in scalars {
        let api = normalize(&property_spec(&format!(
            "{{type: string, minLength: 1, maxLength: 60, pattern: '^.*$', x-satay: {{parse-as: {wire}}}}}"
        )));
        let record = object(&definition(&api, "Record").schema);
        let value = &record.properties[0].value;
        let parsed = string(value);
        assert_eq!(parsed.interpretation, StringInterpretation::Scalar(scalar));
        assert_eq!(parsed.constraints.min_length, Some(1));
        assert_eq!(parsed.constraints.max_length, Some(60));
        assert_eq!(parsed.constraints.pattern.as_deref(), Some("^.*$"));
        assert_eq!(
            value.annotations.source.as_ref().unwrap().pointer,
            "/components/schemas/Record/properties/value~1~0"
        );
    }
}

#[test]
fn formats_do_not_infer_interpretations_or_integer_widths() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Formats, version: '1'}
paths: {}
components:
  schemas:
    Formats:
      type: object
      properties:
        url: {type: string, format: uri}
        explicit: {type: string, format: uri, x-satay: {parse-as: date}}
        timestamp: {type: integer, format: unixtime}
        integerBool: {type: integer, format: int64, x-satay: {parse-as: bool}}
        number: {type: number, format: float}
"#,
    );
    let properties = &object(&definition(&api, "Formats").schema).properties;
    let url = &property(properties, "url").value;
    assert_eq!(string(url).interpretation, StringInterpretation::Plain);
    assert_eq!(url.annotations.format.as_deref(), Some("uri"));
    let explicit = &property(properties, "explicit").value;
    assert_eq!(
        string(explicit).interpretation,
        StringInterpretation::Scalar(StringScalar::Date)
    );
    assert_eq!(explicit.annotations.format.as_deref(), Some("uri"));
    let timestamp = &property(properties, "timestamp").value;
    let TypeExpr::Integer(integer) = &timestamp.ty else {
        panic!("integer")
    };
    assert_eq!(
        integer.interpretation,
        IntegerInterpretation::Numeric {
            representation: None
        }
    );
    assert_eq!(timestamp.annotations.format.as_deref(), Some("unixtime"));
    let integer_bool = &property(properties, "integerBool").value;
    let TypeExpr::Integer(integer) = &integer_bool.ty else {
        panic!("integer")
    };
    assert_eq!(integer.interpretation, IntegerInterpretation::Bool);
    assert_eq!(integer_bool.annotations.format.as_deref(), Some("int64"));
    let number = &property(properties, "number").value;
    assert!(matches!(number.ty, TypeExpr::Number(_)));
    assert_eq!(number.annotations.format.as_deref(), Some("float"));
}

#[test]
fn ranges_keep_representation_intent_numeric_bounds_and_wire_constraints_separate() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Ranges, version: '1'}
paths: {}
components:
  schemas:
    Ranges:
      type: object
      properties:
        absent: {type: string, x-satay: {parse-as: integer-range}}
        auto: {type: string, x-satay: {parse-as: integer-range, integer-type: auto}}
        narrow:
          type: string
          format: int64
          minLength: 2
          maxLength: 20
          pattern: '^[0-9-]+$'
          minimum: 256
          maximum: 512
          x-satay: {parse-as: integer-range, integer-type: u8}
        floating:
          type: string
          format: double
          minimum: -1.5
          exclusiveMaximum: 3.25
          x-satay: {parse-as: number-range}
"#,
    );
    let properties = &object(&definition(&api, "Ranges").schema).properties;
    for (name, representation) in [
        ("absent", None),
        ("auto", Some(IntegerRepresentation::Auto)),
    ] {
        assert_eq!(
            string(&property(properties, name).value).interpretation,
            StringInterpretation::IntegerRange {
                representation,
                bounds: NumericConstraints::default(),
            }
        );
    }
    let narrow = &property(properties, "narrow").value;
    let narrow_string = string(narrow);
    assert_eq!(
        narrow_string.interpretation,
        StringInterpretation::IntegerRange {
            representation: Some(IntegerRepresentation::U8),
            bounds: NumericConstraints {
                declared: None,
                minimum: Some(NumericBound {
                    value: Number::from(256),
                    exclusive: false
                }),
                maximum: Some(NumericBound {
                    value: Number::from(512),
                    exclusive: false
                }),
            },
        }
    );
    assert_eq!(narrow.annotations.format.as_deref(), Some("int64"));
    assert_eq!(narrow_string.constraints.min_length, Some(2));
    assert_eq!(narrow_string.constraints.max_length, Some(20));
    assert_eq!(
        narrow_string.constraints.pattern.as_deref(),
        Some("^[0-9-]+$")
    );
    let floating = &property(properties, "floating").value;
    assert_eq!(
        string(floating).interpretation,
        StringInterpretation::NumberRange {
            bounds: NumericConstraints {
                declared: None,
                minimum: Some(NumericBound {
                    value: Number::from_f64(-1.5).unwrap(),
                    exclusive: false
                }),
                maximum: Some(NumericBound {
                    value: Number::from_f64(3.25).unwrap(),
                    exclusive: true
                }),
            },
        }
    );
    assert_eq!(floating.annotations.format.as_deref(), Some("double"));
}

#[test]
fn enum_names_remain_requested_spellings_in_wire_map_order_without_rust_collisions() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Names, version: '1'}
paths: {}
components:
  schemas:
    Names:
      type: string
      enum: [z, b, a, c]
      x-satay:
        enum-variants: {z: Other, b: same-name, a: same_name, c: same-name}
    Single:
      type: string
      const: selected
      x-satay:
        enum-variants: {selected: 'keep THIS spelling'}
"#,
    );
    let names = string(&definition(&api, "Names").schema);
    assert_eq!(names.enum_values.as_ref().unwrap(), &["z", "b", "a", "c"]);
    assert_eq!(
        names
            .enum_variants
            .iter()
            .map(|name| (name.wire_value.as_str(), name.requested_name.as_str()))
            .collect::<Vec<_>>(),
        [
            ("a", "same_name"),
            ("b", "same-name"),
            ("c", "same-name"),
            ("z", "Other")
        ]
    );
    let single = string(&definition(&api, "Single").schema);
    assert_eq!(single.const_value.as_deref(), Some("selected"));
    assert_eq!(single.enum_variants[0].requested_name, "keep THIS spelling");
}

#[test]
fn boolean_mappings_sentinels_and_property_policies_retain_exact_values() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Policies, version: '1'}
paths: {}
components:
  schemas:
    Reading:
      type: object
      required: [mapped, ignored]
      properties:
        mapped:
          type: string
          x-satay:
            parse-as: bool
            true-values: [Y, Yes, Y, '1']
            false-values: [N, '0', '']
            unknown-as: false
            none-if: [NA, '-', NA]
            treat-error-as-none: false
            identifier: http-status
        sentinel:
          type: string
          x-satay: {parse-as: f64, none-if: ['', NA, '']}
        strict:
          type: string
          x-satay: {parse-as: f64, ignore: false, treat-error-as-none: false}
        loose:
          type: string
          x-satay: {parse-as: f64, treat-error-as-none: true}
        ignored:
          type: object
          required: [payload]
          properties:
            payload: {type: string, description: retained}
          additionalProperties: false
          x-satay: {ignore: true}
"#,
    );
    let properties = &object(&definition(&api, "Reading").schema).properties;
    let mapped = property(properties, "mapped");
    let StringInterpretation::MappedBool(mapping) = &string(&mapped.value).interpretation else {
        panic!("mapped boolean")
    };
    assert_eq!(mapping.true_values(), ["Y", "Yes", "Y", "1"]);
    assert_eq!(mapping.false_values(), ["N", "0", ""]);
    assert_eq!(mapping.unknown_as(), Some(false));
    let PropertyPolicy::Included {
        identifier,
        decoding: DecodePolicy::SentinelAsAbsent(values),
    } = &mapped.policy
    else {
        panic!("sentinel policy")
    };
    assert_eq!(identifier.as_ref().unwrap(), &["http", "status"]);
    assert_eq!(values.values(), ["NA", "-", "NA"]);
    assert!(mapped.required);
    let sentinel = property(properties, "sentinel");
    let PropertyPolicy::Included {
        decoding: DecodePolicy::SentinelAsAbsent(values),
        ..
    } = &sentinel.policy
    else {
        panic!("sentinel policy")
    };
    assert_eq!(values.values(), ["", "NA", ""]);
    assert!(!sentinel.required);
    assert_eq!(
        property(properties, "strict").policy,
        PropertyPolicy::default()
    );
    assert_eq!(
        property(properties, "loose").policy,
        PropertyPolicy::Included {
            identifier: None,
            decoding: DecodePolicy::ErrorAsAbsent,
        }
    );
    let ignored = property(properties, "ignored");
    assert!(ignored.required);
    assert_eq!(ignored.policy, PropertyPolicy::Ignored);
    let wire = object(&ignored.value);
    assert_eq!(wire.additional_properties, AdditionalProperties::Forbidden);
    assert_eq!(wire.properties[0].wire_name, "payload");
    assert!(wire.properties[0].required);
    assert_eq!(
        wire.properties[0].value.annotations.description.as_deref(),
        Some("retained")
    );
    assert_eq!(
        wire.properties[0]
            .value
            .annotations
            .source
            .as_ref()
            .unwrap()
            .pointer,
        "/components/schemas/Reading/properties/ignored/properties/payload"
    );
}

#[test]
fn shared_coordinate_definitions_keep_declared_alias_identity_and_independent_local_policies() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Coordinates, version: '1'}
paths: {}
components:
  schemas:
    Uses:
      type: object
      required: [strict, loose]
      properties:
        strict: {$ref: '#/components/schemas/Packed'}
        loose:
          $ref: '#/components/schemas/Packed'
          x-satay: {treat-error-as-none: true, identifier: gps-location}
        sentinel:
          type: string
          x-satay:
            parse-as: coordinates
            target: {$ref: '#/components/schemas/Point~1Alias~0'}
            fields: [north, east]
            delimiter: ', '
            none-if: ['', NA]
    Packed:
      type: string
      description: packed value
      x-satay:
        parse-as: coordinates
        target: {$ref: '#/components/schemas/Point~1Alias~0'}
        fields: [north, east]
    'Point/Alias~': {$ref: '#/components/schemas/PointWrapper'}
    PointWrapper:
      description: coordinate target annotation
      allOf: [{$ref: '#/components/schemas/Point'}]
    Point:
      allOf:
        - {$ref: '#/components/schemas/EastPart'}
        - {$ref: '#/components/schemas/NorthPart'}
    EastPart:
      allOf: [{$ref: '#/components/schemas/EastBase'}]
    EastBase:
      type: object
      required: [east]
      properties:
        east:
          $ref: '#/components/schemas/NumberAlias'
          x-satay: {ignore: false, treat-error-as-none: false}
    NorthPart:
      type: object
      required: [north]
      properties:
        north: {type: number, minimum: -90, maximum: 90}
    NumberAlias: {$ref: '#/components/schemas/NumberWrapper'}
    NumberWrapper:
      description: bounded number annotation
      allOf: [{$ref: '#/components/schemas/BoundedNumber'}]
    BoundedNumber: {type: number, format: double, minimum: -180, maximum: 180}
"#,
    );
    let StringInterpretation::Coordinates(coordinates) =
        &string(&definition(&api, "Packed").schema).interpretation
    else {
        panic!("coordinate interpretation")
    };
    assert_eq!(
        api.definition(coordinates.target()).unwrap().source_name,
        "Point/Alias~"
    );
    assert_eq!(coordinates.fields(), &["north", "east"]);
    assert_eq!(coordinates.delimiter(), " ");
    assert!(matches!(
        definition(&api, "Point/Alias~").schema.ty,
        TypeExpr::Ref(_)
    ));
    let TypeExpr::Composition(point) = &definition(&api, "Point").schema.ty else {
        panic!("allOf")
    };
    assert_eq!(point.kind, CompositionKind::AllOf);
    let TypeExpr::Ref(east_id) = point.branches[0].ty else {
        panic!("branch ref")
    };
    assert_eq!(api.definition(east_id).unwrap().source_name, "EastPart");
    let uses = &object(&definition(&api, "Uses").schema).properties;
    let strict = property(uses, "strict");
    let loose = property(uses, "loose");
    assert_eq!(strict.value.ty, loose.value.ty);
    let TypeExpr::Ref(packed_id) = strict.value.ty else {
        panic!("shared ref")
    };
    assert_eq!(api.definition(packed_id).unwrap().source_name, "Packed");
    assert_eq!(strict.policy, PropertyPolicy::default());
    assert_eq!(
        loose.policy,
        PropertyPolicy::Included {
            identifier: Some(vec!["gps".to_owned(), "location".to_owned()]),
            decoding: DecodePolicy::ErrorAsAbsent,
        }
    );
    assert_eq!(strict.value.annotations.description, None);
    assert_eq!(loose.value.annotations.description, None);
    let sentinel = property(uses, "sentinel");
    let StringInterpretation::Coordinates(local) = &string(&sentinel.value).interpretation else {
        panic!("local coordinates")
    };
    assert_eq!(local.target(), coordinates.target());
    assert_eq!(local.fields(), coordinates.fields());
    assert_eq!(local.delimiter(), ", ");
    assert!(!sentinel.value.nullable);
    let PropertyPolicy::Included {
        decoding: DecodePolicy::SentinelAsAbsent(values),
        ..
    } = &sentinel.policy
    else {
        panic!("coordinate sentinel")
    };
    assert_eq!(values.values(), ["", "NA"]);
    let TypeExpr::Number(number) = &definition(&api, "BoundedNumber").schema.ty else {
        panic!("number")
    };
    assert_eq!(
        number.constraints.minimum.as_ref().unwrap().value,
        Number::from(-180)
    );
    assert_eq!(
        number.constraints.maximum.as_ref().unwrap().value,
        Number::from(180)
    );
}

#[test]
fn scalar_reference_policies_do_not_mutate_the_shared_codec_or_inherit_annotations() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Locality, version: '1'}
paths: {}
components:
  schemas:
    Dates:
      type: object
      properties:
        strict: {$ref: '#/components/schemas/Date'}
        loose:
          $ref: '#/components/schemas/Date'
          description: local
          x-satay: {treat-error-as-none: true}
    Date:
      type: string
      description: target
      default: '2025-01-01'
      x-satay: {parse-as: date}
"#,
    );
    let dates = &object(&definition(&api, "Dates").schema).properties;
    assert_eq!(dates[0].value.ty, dates[1].value.ty);
    assert_eq!(dates[0].policy, PropertyPolicy::default());
    assert_eq!(
        dates[1].policy,
        PropertyPolicy::Included {
            identifier: None,
            decoding: DecodePolicy::ErrorAsAbsent,
        }
    );
    assert_eq!(dates[0].value.annotations.description, None);
    assert_eq!(
        dates[1].value.annotations.description.as_deref(),
        Some("local")
    );
    assert_eq!(dates[0].value.annotations.default, None);
    assert_eq!(dates[1].value.annotations.default, None);
    let date = &definition(&api, "Date").schema;
    assert_eq!(
        string(date).interpretation,
        StringInterpretation::Scalar(StringScalar::Date)
    );
    assert_eq!(date.annotations.description.as_deref(), Some("target"));
    assert_eq!(
        date.annotations.default.as_ref().unwrap().as_str(),
        Some("2025-01-01")
    );
}

#[test]
fn enum_mapping_membership_uses_effective_wire_values_and_escaped_locations() {
    let schema = "{type: string, enum: [ok], x-satay: {enum-variants: {'bad/~': Requested}}}";
    assert!(matches!(
        validation_at(&property_spec(schema), "/components/schemas/Record/properties/value~1~0/x-satay/enum-variants/bad~1~0"),
        ValidationError::UnknownSatayEnumVariantValue { wire_name, .. } if wire_name == "bad/~"
    ));
    let schema = "{type: string, enum: [a, b], const: a, x-satay: {enum-variants: {b: B}}}";
    assert!(matches!(
        validation_at(&property_spec(schema), "/components/schemas/Record/properties/value~1~0/x-satay/enum-variants/b"),
        ValidationError::UnknownSatayEnumVariantValue { wire_name, .. } if wire_name == "b"
    ));
}

#[test]
fn enum_interpretation_options_and_missing_enum_members_are_rejected() {
    assert!(matches!(
        property_error("{type: string, enum: [a], x-satay: {parse-as: bool}}"),
        ValidationError::SatayParseAsWithEnum { parse_as, .. } if parse_as == "bool"
    ));
    for keyword in ["integer-type: u8", "true-values: [Y]", "fields: [x, y]"] {
        let expected = keyword.split(':').next().unwrap();
        assert!(matches!(
            property_error(&format!("{{type: string, enum: [a], x-satay: {{{keyword}}}}}")),
            ValidationError::SatayOptionUnsupportedWithEnum { keyword, .. } if keyword == expected
        ));
    }
    assert!(matches!(
        property_error("{type: string, x-satay: {enum-variants: {a: A}}}"),
        ValidationError::SatayEnumVariantsRequireEnum { .. }
    ));
}

#[test]
fn sentinel_empty_kind_and_conflict_errors_are_typed_at_the_extension() {
    assert!(matches!(
        property_error("{type: string, x-satay: {parse-as: f64, none-if: []}}"),
        ValidationError::EmptySatayNoneIf { .. }
    ));
    assert!(matches!(
        property_error("{type: string, x-satay: {none-if: [NA]}}"),
        ValidationError::SatayNoneIfRequiresParsedString { .. }
    ));
    assert!(matches!(
        property_error("{type: string, x-satay: {parse-as: integer-range, none-if: [NA]}}"),
        ValidationError::SatayNoneIfRequiresParsedString { .. }
    ));
    assert!(matches!(
        property_error("{type: integer, x-satay: {parse-as: bool, none-if: [NA]}}"),
        ValidationError::SatayNoneIfRequiresParsedString { .. }
    ));
    assert!(matches!(
        property_error(
            "{type: string, x-satay: {parse-as: date, none-if: [NA], treat-error-as-none: true}}"
        ),
        ValidationError::ConflictingSatayNoneHandling { .. }
    ));
}

#[test]
fn mapping_errors_distinguish_missing_empty_and_overlapping_values() {
    for mapping in ["true-values: [Y]", "unknown-as: false"] {
        assert!(matches!(
            property_error(&format!(
                "{{type: string, x-satay: {{parse-as: bool, {mapping}}}}}"
            )),
            ValidationError::IncompleteSatayBoolMapping { .. }
        ));
    }
    assert!(matches!(
        property_error(
            "{type: integer, x-satay: {parse-as: bool, true-values: [Y], false-values: [N]}}"
        ),
        ValidationError::SatayBoolMappingRequiresParsedStringBool { .. }
    ));
    for (mapping, expected) in [
        ("true-values: [], false-values: [N]", "true-values"),
        ("true-values: [Y], false-values: []", "false-values"),
    ] {
        assert!(matches!(
            property_error(&format!("{{type: string, x-satay: {{parse-as: bool, {mapping}}}}}")),
            ValidationError::EmptySatayBoolMapping { keyword, .. } if keyword == expected
        ));
    }
    assert!(matches!(
        property_error("{type: string, x-satay: {parse-as: bool, true-values: [second, first], false-values: [first, second]}}"),
        ValidationError::OverlappingSatayBoolMapping { value, .. } if value == "second"
    ));
    assert!(matches!(
        property_error("{type: string, x-satay: {parse-as: bool, true-values: [Y], false-values: [N], none-if: [N, Y]}}"),
        ValidationError::OverlappingSatayBoolMappingNoneIf { value, .. } if value == "N"
    ));
}

#[test]
fn ignore_conflicts_include_false_valued_options() {
    for (other, expected) in [
        ("treat-error-as-none: false", "treat-error-as-none"),
        ("parse-as: bool", "parse-as"),
        ("identifier: replacement", "identifier"),
        ("enum-variants: {a: A}", "enum-variants"),
    ] {
        assert!(matches!(
            property_error(&format!("{{type: string, x-satay: {{ignore: true, {other}}}}}")),
            ValidationError::SatayOptionConflictsWithIgnore { keyword, .. } if keyword == expected
        ));
    }
}

#[test]
fn value_placement_preserves_false_valued_property_option_presence() {
    let error = |options: &str| {
        validation_at(
            &format!(
                "openapi: 3.1.0\ninfo: {{title: Placement, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    Value: {{type: string, x-satay: {{{options}}}}}\n"
            ),
            "/components/schemas/Value/x-satay",
        )
    };
    assert!(matches!(
        error("ignore: false"),
        ValidationError::SatayIgnoreRequiresObjectProperty { .. }
    ));
    assert!(matches!(
        error("treat-error-as-none: false"),
        ValidationError::SatayTreatErrorAsNoneRequiresObjectProperty { .. }
    ));
    assert!(matches!(
        error("parse-as: date, none-if: [NA]"),
        ValidationError::SatayNoneIfRequiresStructField { .. }
    ));
    assert!(matches!(
        error("identifier: value"),
        ValidationError::SatayIdentifierRequiresObjectProperty { .. }
    ));
}

#[test]
fn reference_codec_overrides_are_rejected_in_the_existing_keyword_order() {
    for (options, expected) in [
        ("parse-as: bool, none-if: [NA]", "x-satay.parse-as"),
        ("none-if: [NA]", "x-satay.none-if"),
        ("integer-type: auto", "x-satay.integer-type"),
        ("true-values: [Y], false-values: [N]", "x-satay.true-values"),
    ] {
        let spec = format!(
            r#"
openapi: 3.1.0
info: {{title: Locality, version: '1'}}
paths: {{}}
components:
  schemas:
    Uses:
      type: object
      properties:
        value:
          $ref: '#/components/schemas/Parsed'
          x-satay: {{{options}}}
    Parsed: {{type: string, x-satay: {{parse-as: bool}}}}
"#
        );
        assert!(matches!(
            validation_at(&spec, "/components/schemas/Uses/properties/value/x-satay"),
            ValidationError::UnsupportedRefSiblingKeyword { keyword, .. } if keyword == expected
        ));
    }
}

#[test]
fn integer_hints_validate_local_meaning_without_selecting_a_width() {
    assert!(matches!(
        property_error("{type: integer, x-satay: {parse-as: bool, integer-type: auto}}"),
        ValidationError::SatayParseAsBoolWithIntegerType { integer_type, .. } if integer_type == "auto"
    ));
    assert!(matches!(
        property_error("{type: string, x-satay: {parse-as: u8, integer-type: u16}}"),
        ValidationError::SatayIntegerTypeRequiresInteger { integer_type, kind, .. }
            if integer_type == "u16" && kind == "string"
    ));
    assert!(matches!(
        property_error("{type: number, x-satay: {parse-as: f64}}"),
        ValidationError::SatayParseAsRequiresString { parse_as, kind, .. }
            if parse_as == "f64" && kind == "number"
    ));
}

fn coordinate_spec(target: &str, selector: &str) -> String {
    format!(
        r#"
openapi: 3.1.0
info: {{title: Coordinates, version: '1'}}
paths: {{}}
components:
  schemas:
    Packed: {{type: string, x-satay: {{{selector}}}}}
    Point: {target}
"#
    )
}

const COORDINATE_SELECTOR: &str =
    "parse-as: coordinates, target: {$ref: '#/components/schemas/Point'}, fields: [x, y]";
const COORDINATE_TARGET: &str =
    "{type: object, required: [x, y], properties: {x: {type: number}, y: {type: number}}}";

#[test]
fn coordinate_selector_errors_point_to_the_declared_extension() {
    for selector in [
        "parse-as: coordinates, fields: [x, y]",
        "parse-as: coordinates, target: {$ref: '#/components/schemas/Point'}",
        "parse-as: coordinates, target: {$ref: '#/components/schemas/Point'}, fields: [x]",
        "parse-as: coordinates, target: {$ref: '#/components/schemas/Point'}, fields: [x, x]",
        "parse-as: coordinates, target: {$ref: '#/components/schemas/Point'}, fields: [x, missing]",
        "parse-as: coordinates, target: {$ref: '#/components/schemas/Point'}, fields: [x, y], delimiter: ''",
        "parse-as: coordinates, target: {$ref: '#/components/schemas/Point'}, fields: [x, y], integer-type: auto",
    ] {
        assert!(matches!(
            validation_at(
                &coordinate_spec(COORDINATE_TARGET, selector),
                "/components/schemas/Packed/x-satay"
            ),
            ValidationError::InvalidSatayCoordinates { .. }
        ));
    }
    assert!(matches!(
        property_error("{type: string, x-satay: {fields: [x, y]}}"),
        ValidationError::SatayOptionRequiresCoordinates {
            keyword: "fields",
            ..
        }
    ));
}

#[test]
fn coordinate_targets_require_exactly_two_required_included_strict_nonnullable_numbers() {
    for target in [
        "{type: [object, 'null'], required: [x, y], properties: {x: {type: number}, y: {type: number}}}",
        "{type: object, required: [x], properties: {x: {type: number}, y: {type: number}}}",
        "{type: object, required: [x, y], properties: {x: {type: number}, y: {type: integer}}}",
        "{type: object, required: [x, y], properties: {x: {type: number}, y: {type: [number, 'null']}}}",
        "{type: object, required: [x, y], properties: {x: {type: number}, y: {type: string, x-satay: {parse-as: f64}}}}",
        "{type: object, required: [x, y], properties: {x: {type: number}, y: {type: number, x-satay: {ignore: true}}}}",
        "{type: object, required: [x, y], properties: {x: {type: number}, y: {type: number, x-satay: {treat-error-as-none: true}}}}",
        "{type: object, required: [x, y], properties: {x: {type: number}, y: {type: number}, z: {type: number}}}",
        "{type: object, required: [x, y], properties: {x: {type: number}, y: {type: number, anyOf: [{type: number}, {type: 'null'}]}}}",
    ] {
        assert!(matches!(
            validation_at(
                &coordinate_spec(target, COORDINATE_SELECTOR),
                "/components/schemas/Packed/x-satay"
            ),
            ValidationError::InvalidSatayCoordinates { .. }
        ));
    }
}

#[test]
fn coordinate_queries_terminate_on_recursive_all_of_targets() {
    let target = "{allOf: [{$ref: '#/components/schemas/Point'}, {type: object, required: [x, y], properties: {x: {type: number}, y: {type: number}}}]}";
    assert!(matches!(
        validation_at(
            &coordinate_spec(target, COORDINATE_SELECTOR),
            "/components/schemas/Packed/x-satay"
        ),
        ValidationError::InvalidSatayCoordinates { .. }
    ));
}

#[test]
fn coordinate_wire_keyword_restrictions_remain_semantic() {
    let spec = format!(
        r#"
openapi: 3.1.0
info: {{title: Coordinates, version: '1'}}
paths: {{}}
components:
  schemas:
    Packed:
      type: string
      minLength: 1
      x-satay: {{{COORDINATE_SELECTOR}}}
    Point: {COORDINATE_TARGET}
"#
    );
    assert!(matches!(
        validation_at(&spec, "/components/schemas/Packed/x-satay"),
        ValidationError::InvalidSatayCoordinates { .. }
    ));
}

#[test]
fn coordinate_numeric_field_queries_detect_cycles_through_aliases_and_wrappers() {
    let spec = format!(
        r#"
openapi: 3.1.0
info: {{title: Coordinates, version: '1'}}
paths: {{}}
components:
  schemas:
    Packed: {{type: string, x-satay: {{{COORDINATE_SELECTOR}}}}}
    Point:
      type: object
      required: [x, y]
      properties:
        x: {{type: number}}
        y: {{$ref: '#/components/schemas/NumericAlias'}}
    NumericAlias: {{$ref: '#/components/schemas/NumericWrapper'}}
    NumericWrapper:
      allOf: [{{$ref: '#/components/schemas/NumericAlias'}}]
"#
    );
    assert!(matches!(
        validation_at(&spec, "/components/schemas/Packed/x-satay"),
        ValidationError::InvalidSatayCoordinates { .. }
    ));
}

#[test]
fn malformed_options_retain_typed_extension_paths_at_their_source() {
    for (options, expected_path) in [
        ("parse-as: unsupported", "x-satay.parse-as"),
        ("parse-as: f64, none-if: [NA, 1]", "x-satay.none-if[1]"),
        ("identifier: 'HTTP status'", "x-satay.identifier"),
        ("ignore: not-a-boolean", "x-satay.ignore"),
    ] {
        assert!(matches!(
            property_error(&format!("{{type: string, x-satay: {{{options}}}}}")),
            ValidationError::InvalidExtension { path, .. } if path == expected_path
        ));
    }
}

#[test]
fn reference_schema_siblings_are_checked_before_malformed_codec_options() {
    let spec = r#"
openapi: 3.1.0
info: {title: Reference order, version: '1'}
paths: {}
components:
  schemas:
    Uses:
      type: object
      properties:
        value:
          $ref: '#/components/schemas/Text'
          type: string
          x-satay: {parse-as: invalid}
    Text: {type: string}
"#;
    assert!(matches!(
        validation_at(spec, "/components/schemas/Uses/properties/value"),
        ValidationError::UnsupportedRefSiblingKeyword { keyword, .. } if keyword == "type"
    ));
}
