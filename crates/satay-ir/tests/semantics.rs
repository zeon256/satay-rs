//! Local semantic retention checks over finalized definitions.
//!
use satay_ir::{
    AdditionalProperties, ApiBuilder, Definition, DefinitionId, IntegerSchema, NumericBound,
    NumericConstraints, ObjectSchema, Property, SchemaAnnotations, SchemaUse, SourceRef,
    StringConstraints, StringSchema, TypeExpr,
};
use serde_json::{Number, Value};

fn definition(source_name: &str, schema: SchemaUse) -> Definition {
    Definition {
        source_name: source_name.into(),
        schema,
    }
}

fn source(pointer: &str) -> SourceRef {
    SourceRef {
        document: "semantics.json".into(),
        pointer: pointer.into(),
    }
}

/// Defines the container object exercising local annotation placement.
fn container_definition(shared: DefinitionId) -> Definition {
    definition(
        "Container",
        SchemaUse::new(TypeExpr::Object(ObjectSchema {
            properties: vec![
                Property {
                    wire_name: "required_nullable".into(),
                    required: true,
                    value: SchemaUse {
                        ty: TypeExpr::Ref(shared),
                        nullable: true,
                        annotations: SchemaAnnotations {
                            description: Some("first use".into()),
                            format: Some("first-format".into()),
                            default: Some(Value::Null),
                            source: Some(source("/properties/required_nullable")),
                        },
                    },
                },
                Property {
                    wire_name: "optional_non_null".into(),
                    required: false,
                    value: SchemaUse {
                        ty: TypeExpr::Ref(shared),
                        nullable: false,
                        annotations: SchemaAnnotations {
                            description: Some("second use".into()),
                            format: Some("second-format".into()),
                            default: None,
                            source: Some(source("/properties/optional_non_null")),
                        },
                    },
                },
            ],
            additional_properties: AdditionalProperties::Unspecified,
        })),
    )
}

#[test]
fn required_nullable_default_and_use_annotations_remain_local() {
    let mut builder = ApiBuilder::new();
    let shared = builder.add_definition(definition(
        "Shared",
        SchemaUse {
            ty: TypeExpr::Boolean,
            nullable: false,
            annotations: SchemaAnnotations {
                description: Some("definition description".into()),
                format: Some("definition-format".into()),
                default: Some(Value::Bool(true)),
                source: Some(source("/$defs/Shared")),
            },
        },
    ));
    let container = builder.add_definition(container_definition(shared));

    let api = builder.finish().unwrap();
    let TypeExpr::Object(container_schema) = &api.definition(container).unwrap().schema.ty else {
        panic!("Container should be an object");
    };
    let required = &container_schema.properties[0];
    let optional = &container_schema.properties[1];

    assert!(required.required);
    assert!(required.value.nullable);
    assert_eq!(required.value.annotations.default, Some(Value::Null));
    assert_eq!(
        required.value.annotations.description.as_deref(),
        Some("first use")
    );
    assert_eq!(
        required.value.annotations.format.as_deref(),
        Some("first-format")
    );
    assert_eq!(
        required.value.annotations.source,
        Some(source("/properties/required_nullable"))
    );

    assert!(!optional.required);
    assert!(!optional.value.nullable);
    assert_eq!(optional.value.annotations.default, None);
    assert_eq!(
        optional.value.annotations.description.as_deref(),
        Some("second use")
    );
    assert_eq!(
        optional.value.annotations.format.as_deref(),
        Some("second-format")
    );
    assert_eq!(
        optional.value.annotations.source,
        Some(source("/properties/optional_non_null"))
    );
    assert!(matches!(required.value.ty, TypeExpr::Ref(id) if id == shared));
    assert!(matches!(optional.value.ty, TypeExpr::Ref(id) if id == shared));

    let shared_schema = &api.definition(shared).unwrap().schema;
    assert!(!shared_schema.nullable);
    assert_eq!(
        shared_schema.annotations.description.as_deref(),
        Some("definition description")
    );
    assert_eq!(
        shared_schema.annotations.format.as_deref(),
        Some("definition-format")
    );
    assert_eq!(shared_schema.annotations.default, Some(Value::Bool(true)));
    assert_eq!(
        shared_schema.annotations.source,
        Some(source("/$defs/Shared"))
    );
}

#[test]
fn numeric_bounds_retain_json_numbers_and_exclusivity() {
    let mut builder = ApiBuilder::new();
    let range = builder.add_definition(definition(
        "Range",
        SchemaUse {
            ty: TypeExpr::Integer(IntegerSchema {
                constraints: NumericConstraints {
                    minimum: Some(NumericBound {
                        value: Number::from(0),
                        exclusive: false,
                    }),
                    maximum: Some(NumericBound {
                        value: Number::from(100),
                        exclusive: true,
                    }),
                },
            }),
            nullable: false,
            annotations: SchemaAnnotations {
                format: Some("int32".into()),
                ..SchemaAnnotations::default()
            },
        },
    ));
    let maximum = builder.add_definition(definition(
        "Maximum",
        SchemaUse::new(TypeExpr::Integer(IntegerSchema {
            constraints: NumericConstraints {
                minimum: None,
                maximum: Some(NumericBound {
                    value: Number::from(u64::MAX),
                    exclusive: false,
                }),
            },
        })),
    ));

    let api = builder.finish().unwrap();
    let range_use = &api.definition(range).unwrap().schema;
    let TypeExpr::Integer(range_schema) = &range_use.ty else {
        panic!("Range should be an integer");
    };
    let minimum = range_schema.constraints.minimum.as_ref().unwrap();
    let upper = range_schema.constraints.maximum.as_ref().unwrap();
    assert_eq!(minimum.value, Number::from(0));
    assert!(!minimum.exclusive);
    assert_eq!(upper.value, Number::from(100));
    assert!(upper.exclusive);
    assert_eq!(range_use.annotations.format.as_deref(), Some("int32"));

    let TypeExpr::Integer(maximum_schema) = &api.definition(maximum).unwrap().schema.ty else {
        panic!("Maximum should be an integer");
    };
    let retained = maximum_schema.constraints.maximum.as_ref().unwrap();
    assert_eq!(retained.value.as_u64(), Some(u64::MAX));
    assert!(!retained.exclusive);
}

#[test]
fn enum_const_and_additional_property_rules_remain_distinct() {
    let mut builder = ApiBuilder::new();
    let string = builder.add_definition(definition(
        "Status",
        SchemaUse::new(TypeExpr::String(StringSchema {
            constraints: StringConstraints::default(),
            enum_values: Some(vec!["pending".into(), "ready".into(), "failed".into()]),
            const_value: Some("ready".into()),
        })),
    ));
    let unspecified = builder.add_definition(object_definition(
        "Unspecified",
        AdditionalProperties::Unspecified,
    ));
    let allowed =
        builder.add_definition(object_definition("Allowed", AdditionalProperties::Allowed));
    let forbidden = builder.add_definition(object_definition(
        "Forbidden",
        AdditionalProperties::Forbidden,
    ));
    let typed = builder.add_definition(object_definition(
        "Typed",
        AdditionalProperties::Schema(Box::new(SchemaUse {
            ty: TypeExpr::String(StringSchema::default()),
            nullable: true,
            annotations: SchemaAnnotations {
                source: Some(source("/additionalProperties")),
                ..SchemaAnnotations::default()
            },
        })),
    ));

    let api = builder.finish().unwrap();
    let TypeExpr::String(string_schema) = &api.definition(string).unwrap().schema.ty else {
        panic!("Status should be a string");
    };
    assert_eq!(
        string_schema.enum_values.as_deref(),
        Some(["pending".into(), "ready".into(), "failed".into()].as_slice())
    );
    assert_eq!(string_schema.const_value.as_deref(), Some("ready"));

    assert!(matches!(
        object_rule(&api.definition(unspecified).unwrap().schema),
        AdditionalProperties::Unspecified
    ));
    assert!(matches!(
        object_rule(&api.definition(allowed).unwrap().schema),
        AdditionalProperties::Allowed
    ));
    assert!(matches!(
        object_rule(&api.definition(forbidden).unwrap().schema),
        AdditionalProperties::Forbidden
    ));
    let AdditionalProperties::Schema(value) = object_rule(&api.definition(typed).unwrap().schema)
    else {
        panic!("Typed should have a typed additional-property rule");
    };
    assert!(value.nullable);
    assert!(matches!(value.ty, TypeExpr::String(_)));
    assert_eq!(
        value.annotations.source,
        Some(source("/additionalProperties"))
    );
}

fn object_definition(source_name: &str, rule: AdditionalProperties) -> Definition {
    definition(
        source_name,
        SchemaUse::new(TypeExpr::Object(ObjectSchema {
            properties: vec![],
            additional_properties: rule,
        })),
    )
}

fn object_rule(schema: &SchemaUse) -> &AdditionalProperties {
    let TypeExpr::Object(object) = &schema.ty else {
        panic!("schema should be an object");
    };
    &object.additional_properties
}
