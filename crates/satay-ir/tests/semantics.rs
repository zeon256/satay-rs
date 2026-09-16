//! Local semantic retention checks over finalized definitions.
//!
use satay_ir::{
    AdditionalProperties, ApiBuilder, BoolMapping, CoordinatesInterpretation, DecodePolicy,
    Definition, DefinitionId, IntegerInterpretation, IntegerRepresentation, IntegerSchema,
    NumberSchema, NumericBound, NumericConstraints, ObjectSchema, Property, PropertyPolicy,
    SchemaAnnotations, SchemaUse, SentinelValues, SourceRef, StringConstraints,
    StringInterpretation, StringSchema, TypeExpr,
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
                            const_value: None,
                            description: Some("first use".into()),
                            format: Some("first-format".into()),
                            default: Some(Value::Null),
                            source: Some(source("/properties/required_nullable")),
                        },
                    },
                    policy: PropertyPolicy::default(),
                },
                Property {
                    wire_name: "optional_non_null".into(),
                    required: false,
                    value: SchemaUse {
                        ty: TypeExpr::Ref(shared),
                        nullable: false,
                        annotations: SchemaAnnotations {
                            const_value: None,
                            description: Some("second use".into()),
                            format: Some("second-format".into()),
                            default: None,
                            source: Some(source("/properties/optional_non_null")),
                        },
                    },
                    policy: PropertyPolicy::default(),
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
                const_value: None,
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
                    declared: None,
                    minimum: Some(NumericBound {
                        value: Number::from(0),
                        exclusive: false,
                    }),
                    maximum: Some(NumericBound {
                        value: Number::from(100),
                        exclusive: true,
                    }),
                },
                interpretation: IntegerInterpretation::default(),
            }),
            nullable: false,
            annotations: SchemaAnnotations {
                const_value: None,
                format: Some("int32".into()),
                ..SchemaAnnotations::default()
            },
        },
    ));
    let maximum = builder.add_definition(definition(
        "Maximum",
        SchemaUse::new(TypeExpr::Integer(IntegerSchema {
            constraints: NumericConstraints {
                declared: None,
                minimum: None,
                maximum: Some(NumericBound {
                    value: Number::from(u64::MAX),
                    exclusive: false,
                }),
            },
            interpretation: IntegerInterpretation::default(),
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
            enum_variants: vec![],
            interpretation: StringInterpretation::Plain,
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
                const_value: None,
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

/// Shared coordinate definitions keep local property policies independent.
#[test]
fn coordinate_uses_keep_local_policies_independent_of_shared_definition() {
    let mut builder = ApiBuilder::new();
    let target = builder.reserve_definition();
    let coordinates = builder.reserve_definition();
    let container = builder.reserve_definition();

    build_coordinate_shapes(&mut builder, target, coordinates);
    builder
        .define(container, policy_container_definition(target, coordinates))
        .unwrap();

    let api = builder.finish().unwrap();
    let TypeExpr::Object(container_schema) = &api.definition(container).unwrap().schema.ty else {
        panic!("Container should be an object");
    };

    // The shared definition is unmodified by any property policy.
    let shared = &api.definition(coordinates).unwrap().schema;
    let TypeExpr::String(shared_string) = &shared.ty else {
        panic!("Coordinates should be a string");
    };
    let StringInterpretation::Coordinates(shared_coordinates) = &shared_string.interpretation
    else {
        panic!("Coordinates should carry a coordinates interpretation");
    };
    assert!(matches!(shared_coordinates.target(), id if id == target));
    assert_eq!(
        shared_coordinates.fields(),
        &["latitude".to_string(), "longitude".to_string()]
    );
    assert_eq!(shared_coordinates.delimiter(), " ");
    assert_eq!(
        shared.annotations.source,
        Some(source("/$defs/Coordinates"))
    );

    let source_property = &container_schema.properties[0];
    assert!(matches!(source_property.value.ty, TypeExpr::Ref(id) if id == coordinates));
    assert_eq!(
        source_property.value.annotations.default,
        Some(Value::String("N/A".into()))
    );
    assert!(matches!(
        &source_property.policy,
        PropertyPolicy::Included {
            identifier: None,
            decoding: DecodePolicy::ErrorAsAbsent
        }
    ));

    let origin_property = &container_schema.properties[1];
    assert!(!origin_property.required);
    assert!(matches!(
        &origin_property.policy,
        PropertyPolicy::Included {
            identifier: Some(words),
            decoding: DecodePolicy::PropagateError,
        } if words == &vec!["location".to_string(), "of".to_string(), "origin".to_string()]
    ));

    let inline_property = &container_schema.properties[2];
    let TypeExpr::String(inline_string) = &inline_property.value.ty else {
        panic!("inline should be an inline string");
    };
    let StringInterpretation::Coordinates(inline_coordinates) = &inline_string.interpretation
    else {
        panic!("inline should carry a coordinates interpretation");
    };
    assert!(matches!(inline_coordinates.target(), id if id == target));
    assert_eq!(
        inline_coordinates.fields(),
        &["longitude".to_string(), "latitude".to_string()]
    );
    assert_eq!(inline_coordinates.delimiter(), ",");
    assert!(matches!(
        &inline_property.policy,
        PropertyPolicy::Included {
            identifier: None,
            decoding: DecodePolicy::SentinelAsAbsent(sentinels),
        } if sentinels.values()
            == ["unknown".to_string(), "N/A".to_string(), String::new()]
    ));
}

/// An ignored property retains its complete wire schema; mapped booleans and
/// integer representations retain declared intent.
#[test]
fn policies_and_interpretations_retain_declared_intent() {
    let mut builder = ApiBuilder::new();
    let record = builder.add_definition(record_definition());

    let api = builder.finish().unwrap();
    let TypeExpr::Object(record_schema) = &api.definition(record).unwrap().schema.ty else {
        panic!("Record should be an object");
    };

    let ignored = &record_schema.properties[0];
    assert!(matches!(ignored.policy, PropertyPolicy::Ignored));
    // The ignored property retains its complete wire schema and annotations.
    let TypeExpr::String(audit_string) = &ignored.value.ty else {
        panic!("audit should be a string");
    };
    let StringInterpretation::MappedBool(mapping) = &audit_string.interpretation else {
        panic!("audit should carry a mapped-bool interpretation");
    };
    assert_eq!(mapping.true_values(), &["yes".to_string(), "y".to_string()]);
    assert_eq!(mapping.false_values(), &["no".to_string(), "n".to_string()]);
    assert_eq!(mapping.unknown_as(), Some(false));
    assert!(ignored.value.nullable);
    assert_eq!(ignored.value.annotations.default, Some(Value::Bool(false)));
    assert_eq!(
        ignored.value.annotations.format.as_deref(),
        Some("sentinel")
    );

    let TypeExpr::Integer(auto_schema) = &record_schema.properties[1].value.ty else {
        panic!("auto_count should be an integer");
    };
    let TypeExpr::Integer(fixed_schema) = &record_schema.properties[2].value.ty else {
        panic!("fixed_count should be an integer");
    };
    assert_eq!(
        auto_schema.interpretation,
        IntegerInterpretation::Numeric {
            representation: Some(IntegerRepresentation::Auto),
        }
    );
    assert_eq!(
        fixed_schema.interpretation,
        IntegerInterpretation::Numeric {
            representation: Some(IntegerRepresentation::U32),
        }
    );
    // No-representation request remains distinct from Auto and U32.
    assert_ne!(
        auto_schema.interpretation,
        IntegerInterpretation::Numeric {
            representation: None
        }
    );
    assert_ne!(
        fixed_schema.interpretation,
        IntegerInterpretation::Numeric {
            representation: None
        }
    );
}

/// Defines the two-number-field placement target and the shared coordinate string.
fn build_coordinate_shapes(
    builder: &mut ApiBuilder,
    target: DefinitionId,
    coordinates: DefinitionId,
) {
    builder
        .define(
            target,
            definition(
                "Placement",
                SchemaUse::new(TypeExpr::Object(ObjectSchema {
                    properties: vec![
                        Property {
                            wire_name: "latitude".into(),
                            required: true,
                            value: SchemaUse::new(TypeExpr::Number(NumberSchema::default())),
                            policy: PropertyPolicy::default(),
                        },
                        Property {
                            wire_name: "longitude".into(),
                            required: true,
                            value: SchemaUse::new(TypeExpr::Number(NumberSchema::default())),
                            policy: PropertyPolicy::default(),
                        },
                    ],
                    additional_properties: AdditionalProperties::Forbidden,
                })),
            ),
        )
        .unwrap();
    builder
        .define(
            coordinates,
            definition(
                "Coordinates",
                SchemaUse {
                    ty: TypeExpr::String(StringSchema {
                        interpretation: StringInterpretation::Coordinates(
                            CoordinatesInterpretation::new(
                                target,
                                ["latitude".into(), "longitude".into()],
                                " ".into(),
                            )
                            .unwrap(),
                        ),
                        ..StringSchema::default()
                    }),
                    nullable: false,
                    annotations: SchemaAnnotations {
                        const_value: None,
                        source: Some(source("/$defs/Coordinates")),
                        ..SchemaAnnotations::default()
                    },
                },
            ),
        )
        .unwrap();
}

/// Defines the container object with locally-policied coordinate uses.
fn policy_container_definition(target: DefinitionId, coordinates: DefinitionId) -> Definition {
    definition(
        "Container",
        SchemaUse::new(TypeExpr::Object(ObjectSchema {
            properties: vec![
                Property {
                    wire_name: "source".into(),
                    required: true,
                    value: SchemaUse {
                        ty: TypeExpr::Ref(coordinates),
                        nullable: false,
                        annotations: SchemaAnnotations {
                            const_value: None,
                            default: Some(Value::String("N/A".into())),
                            ..SchemaAnnotations::default()
                        },
                    },
                    policy: PropertyPolicy::Included {
                        identifier: None,
                        decoding: DecodePolicy::ErrorAsAbsent,
                    },
                },
                Property {
                    wire_name: "origin".into(),
                    required: false,
                    value: SchemaUse::new(TypeExpr::Ref(coordinates)),
                    policy: PropertyPolicy::Included {
                        identifier: Some(vec!["location".into(), "of".into(), "origin".into()]),
                        decoding: DecodePolicy::PropagateError,
                    },
                },
                Property {
                    wire_name: "inline".into(),
                    required: false,
                    value: SchemaUse {
                        ty: TypeExpr::String(StringSchema {
                            interpretation: StringInterpretation::Coordinates(
                                CoordinatesInterpretation::new(
                                    target,
                                    ["longitude".into(), "latitude".into()],
                                    ",".into(),
                                )
                                .unwrap(),
                            ),
                            ..StringSchema::default()
                        }),
                        nullable: false,
                        annotations: SchemaAnnotations {
                            const_value: None,
                            default: None,
                            ..SchemaAnnotations::default()
                        },
                    },
                    policy: PropertyPolicy::Included {
                        identifier: None,
                        decoding: DecodePolicy::SentinelAsAbsent(
                            SentinelValues::new(vec![
                                "unknown".into(),
                                "N/A".into(),
                                String::new(),
                            ])
                            .unwrap(),
                        ),
                    },
                },
            ],
            additional_properties: AdditionalProperties::Unspecified,
        })),
    )
}

/// Defines the record object with an ignored mapped-bool property and two
/// integer properties carrying distinct representation requests.
fn record_definition() -> Definition {
    definition(
        "Record",
        SchemaUse::new(TypeExpr::Object(ObjectSchema {
            properties: vec![
                Property {
                    wire_name: "audit".into(),
                    required: false,
                    value: SchemaUse {
                        ty: TypeExpr::String(StringSchema {
                            interpretation: StringInterpretation::MappedBool(
                                BoolMapping::new(
                                    vec!["yes".into(), "y".into()],
                                    vec!["no".into(), "n".into()],
                                    Some(false),
                                )
                                .unwrap(),
                            ),
                            ..StringSchema::default()
                        }),
                        nullable: true,
                        annotations: SchemaAnnotations {
                            const_value: None,
                            default: Some(Value::Bool(false)),
                            format: Some("sentinel".into()),
                            ..SchemaAnnotations::default()
                        },
                    },
                    policy: PropertyPolicy::Ignored,
                },
                Property {
                    wire_name: "auto_count".into(),
                    required: true,
                    value: SchemaUse::new(TypeExpr::Integer(IntegerSchema {
                        interpretation: IntegerInterpretation::Numeric {
                            representation: Some(IntegerRepresentation::Auto),
                        },
                        ..IntegerSchema::default()
                    })),
                    policy: PropertyPolicy::default(),
                },
                Property {
                    wire_name: "fixed_count".into(),
                    required: true,
                    value: SchemaUse::new(TypeExpr::Integer(IntegerSchema {
                        interpretation: IntegerInterpretation::Numeric {
                            representation: Some(IntegerRepresentation::U32),
                        },
                        ..IntegerSchema::default()
                    })),
                    policy: PropertyPolicy::default(),
                },
            ],
            additional_properties: AdditionalProperties::Unspecified,
        })),
    )
}
