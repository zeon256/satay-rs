use satay_ir::{IntegerInterpretation, IntegerRepresentation, TypeExpr};
use serde_json::json;

use super::{definition, normalize, object};
use crate::error::ValidationError;
use crate::parse::normalize::{NormalizeError, normalize_spec};

#[test]
fn numeric_bounds_keep_exact_values_tightness_exclusivity_and_hint() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Bounds, version: '1'}
paths: {}
components:
  schemas:
    Bounded: {type: integer, minimum: 0, maximum: 100, format: int32}
    Largest: {type: integer, minimum: 18446744073709551615, maximum: 18446744073709551615}
    Tighter: {type: integer, minimum: 2, exclusiveMinimum: 2, maximum: 10, exclusiveMaximum: 12}
    Floating: {type: number, minimum: 1.5, exclusiveMinimum: 1.25, maximum: 3, exclusiveMaximum: 3}
    Auto: {type: integer, x-satay: {integer-type: auto}}
"#,
    );
    let TypeExpr::Integer(bounded) = &definition(&api, "Bounded").schema.ty else {
        panic!("integer")
    };
    assert_eq!(
        bounded.constraints.minimum.as_ref().unwrap().value,
        0.into()
    );
    assert_eq!(
        bounded.constraints.maximum.as_ref().unwrap().value,
        100.into()
    );
    assert_eq!(
        definition(&api, "Bounded")
            .schema
            .annotations
            .format
            .as_deref(),
        Some("int32")
    );
    assert_eq!(
        bounded.interpretation,
        IntegerInterpretation::Numeric {
            representation: None
        }
    );
    let TypeExpr::Integer(largest) = &definition(&api, "Largest").schema.ty else {
        panic!("integer")
    };
    assert_eq!(
        largest.constraints.minimum.as_ref().unwrap().value.as_u64(),
        Some(u64::MAX)
    );
    assert_eq!(
        largest.constraints.maximum.as_ref().unwrap().value.as_u64(),
        Some(u64::MAX)
    );
    let TypeExpr::Integer(tighter) = &definition(&api, "Tighter").schema.ty else {
        panic!("integer")
    };
    let min = tighter.constraints.minimum.as_ref().unwrap();
    let max = tighter.constraints.maximum.as_ref().unwrap();
    assert_eq!((&min.value, min.exclusive), (&2.into(), true));
    assert_eq!((&max.value, max.exclusive), (&10.into(), false));
    let TypeExpr::Number(floating) = &definition(&api, "Floating").schema.ty else {
        panic!("number")
    };
    assert_eq!(
        floating
            .constraints
            .minimum
            .as_ref()
            .unwrap()
            .value
            .as_f64(),
        Some(1.5)
    );
    assert!(!floating.constraints.minimum.as_ref().unwrap().exclusive);
    assert!(floating.constraints.maximum.as_ref().unwrap().exclusive);
    let TypeExpr::Integer(auto) = &definition(&api, "Auto").schema.ty else {
        panic!("integer")
    };
    assert_eq!(
        auto.interpretation,
        IntegerInterpretation::Numeric {
            representation: Some(IntegerRepresentation::Auto)
        }
    );
}

#[test]
fn semantic_bounds_do_not_clamp_to_a_requested_rust_width() {
    let spec = r#"
openapi: 3.1.0
info: {title: Semantic range, version: '1'}
paths: {}
components:
  schemas:
    Wide: {type: integer, minimum: 256, maximum: 300, x-satay: {integer-type: u8}}
"#;
    let api = normalize(spec);
    let TypeExpr::Integer(wide) = &definition(&api, "Wide").schema.ty else {
        panic!("integer")
    };
    assert_eq!(
        wide.interpretation,
        IntegerInterpretation::Numeric {
            representation: Some(IntegerRepresentation::U8)
        }
    );
    assert_eq!(wide.constraints.minimum.as_ref().unwrap().value, 256.into());
    assert_eq!(wide.constraints.maximum.as_ref().unwrap().value, 300.into());

    // Backend representability is independently rejected without excluding the semantic graph.
    assert!(crate::generate(spec).is_err());
}

#[test]
fn empty_declared_integer_and_number_intervals_are_structured_errors() {
    for (schema, integer) in [
        (
            "{type: integer, exclusiveMinimum: 1, exclusiveMaximum: 2}",
            true,
        ),
        ("{type: number, minimum: 1, exclusiveMaximum: 1}", false),
    ] {
        let spec = format!(
            "openapi: 3.1.0\ninfo: {{title: Bounds, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    Empty: {schema}\n"
        );
        let NormalizeError::Validation { location, source } =
            normalize_spec(&spec, "bounds.yaml").unwrap_err()
        else {
            panic!("semantic validation error")
        };

        assert_eq!(location.document, "bounds.yaml");
        assert_eq!(location.pointer, "/components/schemas/Empty");
        assert!(matches!(
            (integer, source.as_ref()),
            (true, ValidationError::EmptyIntegerBounds { .. })
                | (false, ValidationError::EmptyNumberBounds { .. })
        ));
    }
}

#[test]
fn json_and_yaml_preserve_required_nullable_explicit_null_and_absence() {
    let json = json!({
        "openapi": "3.1.0", "info": {"title":"Presence", "version":"1"}, "paths": {},
        "components": {"schemas": {"Record": {"type":"object", "required":["explicit"], "properties": {
            "explicit": {"type":["string","null"], "default":null},
            "absent": {"type":"string"}
        }}}}
    }).to_string();

    let yaml = r#"
openapi: 3.1.0
info: {title: Presence, version: '1'}
paths: {}
components:
  schemas:
    Record:
      type: object
      required: [explicit]
      properties:
        explicit: {type: [string, 'null'], default: null}
        absent: {type: string}
"#;

    let json_api = normalize(&json);
    let yaml_api = normalize(yaml);
    assert_eq!(
        definition(&json_api, "Record"),
        definition(&yaml_api, "Record")
    );

    let record = object(&definition(&json_api, "Record").schema);
    let explicit = record
        .properties
        .iter()
        .find(|p| p.wire_name == "explicit")
        .unwrap();

    let absent = record
        .properties
        .iter()
        .find(|p| p.wire_name == "absent")
        .unwrap();

    assert!(explicit.required && explicit.value.nullable);
    assert_eq!(explicit.value.annotations.default, Some(json!(null)));
    assert!(!absent.required && !absent.value.nullable);
    assert_eq!(absent.value.annotations.default, None);
}

#[test]
fn yaml_aliases_index_null_defaults_at_each_actual_occurrence() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Alias presence, version: '1'}
paths: {}
components:
  schemas:
    Record:
      type: object
      properties:
        first: &nullable {type: [string, 'null'], default: null}
        second: *nullable
"#,
    );

    let record = object(&definition(&api, "Record").schema);

    for field in &record.properties {
        assert_eq!(field.value.annotations.default, Some(json!(null)));
        assert!(field.value.nullable);
        assert_eq!(
            field.value.annotations.source.as_ref().unwrap().pointer,
            format!("/components/schemas/Record/properties/{}", field.wire_name)
        );
    }
}

#[test]
fn length_bounds_are_checked_without_compiling_target_regexes() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Patterns, version: '1'}
paths: {}
components:
  schemas:
    Pattern: {type: string, pattern: '['}
"#,
    );

    assert_eq!(
        super::string(&definition(&api, "Pattern").schema)
            .constraints
            .pattern
            .as_deref(),
        Some("[")
    );

    for (schema, string) in [
        ("{type: string, minLength: 3, maxLength: 2}", true),
        (
            "{type: array, items: {type: boolean}, minItems: 3, maxItems: 2}",
            false,
        ),
    ] {
        let spec = format!(
            "openapi: 3.1.0\ninfo: {{title: Bounds, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    Invalid: {schema}\n"
        );

        let NormalizeError::Validation { location, source } =
            normalize_spec(&spec, "length.yaml").unwrap_err()
        else {
            panic!("length validation")
        };

        assert_eq!(location.pointer, "/components/schemas/Invalid");
        assert!(matches!(
            (string, source.as_ref()),
            (
                true,
                ValidationError::InvalidStringLengthBounds {
                    min_length: 3,
                    max_length: 2,
                    ..
                }
            ) | (
                false,
                ValidationError::InvalidArrayLengthBounds {
                    min_items: 3,
                    max_items: 2,
                    ..
                }
            )
        ));
    }
}
