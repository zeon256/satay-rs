//! Identity, cycle, and error-order checks over the structural graph.
//!
use std::collections::HashSet;

use satay_ir::{
    AdditionalProperties, Api, ApiBuilder, ArrayConstraints, ArraySchema, BuildError,
    CompositionKind, CompositionSchema, Definition, DefinitionId, Discriminator,
    DiscriminatorMapping, GraphOwner, ObjectSchema, Property, PropertyPolicy, SchemaAnnotations,
    SchemaUse, SourceRef, StringInterpretation, StringSchema, TypeExpr,
};

fn definition(source_name: &str, schema: SchemaUse) -> Definition {
    Definition {
        source_name: source_name.into(),
        schema,
    }
}

fn empty_object(source_name: &str) -> Definition {
    definition(
        source_name,
        SchemaUse::new(TypeExpr::Object(ObjectSchema {
            properties: vec![],
            additional_properties: AdditionalProperties::Unspecified,
        })),
    )
}

fn reference(target: DefinitionId, pointer: &str) -> SchemaUse {
    SchemaUse {
        ty: TypeExpr::Ref(target),
        nullable: false,
        annotations: SchemaAnnotations {
            source: Some(SourceRef {
                document: "graph.json".into(),
                pointer: pointer.into(),
            }),
            ..SchemaAnnotations::default()
        },
    }
}

fn references_in(schema: &SchemaUse, references: &mut Vec<DefinitionId>) {
    match &schema.ty {
        TypeExpr::Ref(target) => references.push(*target),
        TypeExpr::Array(array) => references_in(&array.items, references),
        TypeExpr::Object(object) => {
            for property in &object.properties {
                references_in(&property.value, references);
            }
            if let AdditionalProperties::Schema(value) = &object.additional_properties {
                references_in(value, references);
            }
        }
        TypeExpr::Composition(composition) => {
            for branch in &composition.branches {
                references_in(branch, references);
            }
            if let Some(discriminator) = &composition.discriminator {
                for mapping in &discriminator.mappings {
                    references.push(mapping.target);
                }
            }
        }
        TypeExpr::String(schema) => {
            if let StringInterpretation::Coordinates(coordinates) = &schema.interpretation {
                references.push(coordinates.target());
            }
        }
        TypeExpr::Integer(_)
        | TypeExpr::Number(_)
        | TypeExpr::Boolean
        | TypeExpr::Null
        | TypeExpr::AnyJson => {}
    }
}

fn direct_references(api: &Api, id: DefinitionId) -> Vec<DefinitionId> {
    let mut references = vec![];
    references_in(&api.definition(id).unwrap().schema, &mut references);
    references
}

fn reachable_definitions(api: &Api, start: DefinitionId) -> HashSet<DefinitionId> {
    let mut visited = HashSet::new();
    let mut pending = vec![start];

    while let Some(id) = pending.pop() {
        if visited.insert(id) {
            pending.extend(direct_references(api, id));
        }
    }

    visited
}

#[test]
fn forward_references_preserve_ids_order_and_shared_identity() {
    let mut builder = ApiBuilder::new();
    let user = builder.reserve_definition();
    let pet = builder.reserve_definition();
    let pet_list = builder.reserve_definition();

    builder
        .define(
            pet_list,
            definition(
                "PetList",
                SchemaUse::new(TypeExpr::Array(ArraySchema {
                    items: Box::new(SchemaUse::new(TypeExpr::Ref(pet))),
                    constraints: ArrayConstraints::default(),
                })),
            ),
        )
        .unwrap();
    builder
        .define(
            pet,
            definition(
                "Pet",
                SchemaUse::new(TypeExpr::Object(ObjectSchema {
                    properties: vec![
                        Property {
                            wire_name: "owner".into(),
                            required: true,
                            value: SchemaUse::new(TypeExpr::Ref(user)),
                            policy: PropertyPolicy::default(),
                        },
                        Property {
                            wire_name: "reviewer".into(),
                            required: false,
                            value: SchemaUse::new(TypeExpr::Ref(user)),
                            policy: PropertyPolicy::default(),
                        },
                        Property {
                            wire_name: "tags".into(),
                            required: false,
                            value: SchemaUse::new(TypeExpr::Array(ArraySchema {
                                items: Box::new(SchemaUse::new(TypeExpr::String(
                                    StringSchema::default(),
                                ))),
                                constraints: ArrayConstraints::default(),
                            })),
                            policy: PropertyPolicy::default(),
                        },
                    ],
                    additional_properties: AdditionalProperties::Forbidden,
                })),
            ),
        )
        .unwrap();
    builder.define(user, empty_object("User")).unwrap();

    let api = builder.finish().unwrap();

    assert_eq!(api.definition(user).unwrap().source_name, "User");
    assert_eq!(api.definition(pet).unwrap().source_name, "Pet");
    assert_eq!(api.definition(pet_list).unwrap().source_name, "PetList");
    assert_eq!(
        api.definitions()
            .map(|(_, definition)| definition.source_name.as_str())
            .collect::<Vec<_>>(),
        ["User", "Pet", "PetList"]
    );

    let TypeExpr::Object(pet_schema) = &api.definition(pet).unwrap().schema.ty else {
        panic!("Pet should be an object");
    };
    assert_eq!(pet_schema.properties.len(), 3);
    assert_eq!(direct_references(&api, pet), [user, user]);
    assert!(matches!(
        pet_schema.properties[2].value.ty,
        TypeExpr::Array(_)
    ));

    let TypeExpr::Array(list_schema) = &api.definition(pet_list).unwrap().schema.ty else {
        panic!("PetList should be an array");
    };
    assert!(matches!(list_schema.items.ty, TypeExpr::Ref(id) if id == pet));
    assert_eq!(api.definitions().count(), 3);
}

#[test]
fn equal_definitions_remain_distinct_allocations() {
    let mut builder = ApiBuilder::new();
    let first = builder.add_definition(definition("Duplicate", SchemaUse::new(TypeExpr::Boolean)));
    let second = builder.add_definition(definition("Duplicate", SchemaUse::new(TypeExpr::Boolean)));

    assert_ne!(first, second);

    let api = builder.finish().unwrap();
    assert_eq!(api.definition(first), api.definition(second));
    assert_eq!(
        api.definitions().map(|(id, _)| id).collect::<Vec<_>>(),
        [first, second]
    );
}

#[test]
fn self_and_mutual_cycles_retain_edges_and_traverse_once() {
    let mut builder = ApiBuilder::new();
    let recursive = builder.reserve_definition();
    let left = builder.reserve_definition();
    let right = builder.reserve_definition();

    builder
        .define(
            recursive,
            definition("Recursive", SchemaUse::new(TypeExpr::Ref(recursive))),
        )
        .unwrap();
    builder
        .define(
            left,
            definition("Left", SchemaUse::new(TypeExpr::Ref(right))),
        )
        .unwrap();
    builder
        .define(
            right,
            definition("Right", SchemaUse::new(TypeExpr::Ref(left))),
        )
        .unwrap();

    let api = builder.finish().unwrap();

    assert_eq!(direct_references(&api, recursive), [recursive]);
    assert_eq!(direct_references(&api, left), [right]);
    assert_eq!(direct_references(&api, right), [left]);
    assert_eq!(
        reachable_definitions(&api, recursive),
        HashSet::from([recursive])
    );
    assert_eq!(
        reachable_definitions(&api, left),
        HashSet::from([left, right])
    );
}

#[test]
fn missing_definition_is_reported_once_even_when_referenced() {
    let mut builder = ApiBuilder::new();
    let missing = builder.reserve_definition();
    let owner = builder.add_definition(definition("Owner", SchemaUse::new(TypeExpr::Ref(missing))));

    let errors = builder.finish().unwrap_err();

    assert_eq!(
        errors.errors(),
        &[BuildError::MissingDefinition { id: missing }]
    );
    assert_ne!(owner, missing);
}

#[test]
fn failed_definitions_leave_existing_slots_unchanged() {
    let mut builder = ApiBuilder::new();
    let local = builder.reserve_definition();
    builder
        .define(
            local,
            definition("First", SchemaUse::new(TypeExpr::Boolean)),
        )
        .unwrap();

    assert_eq!(
        builder.define(
            local,
            definition("Second", SchemaUse::new(TypeExpr::AnyJson))
        ),
        Err(BuildError::AlreadyDefined { id: local })
    );

    let mut larger_builder = ApiBuilder::new();
    larger_builder.reserve_definition();
    let out_of_range = larger_builder.reserve_definition();
    assert_eq!(
        builder.define(
            out_of_range,
            definition("Foreign", SchemaUse::new(TypeExpr::AnyJson))
        ),
        Err(BuildError::UnknownDefinition { id: out_of_range })
    );

    let api = builder.finish().unwrap();
    assert_eq!(api.definition(local).unwrap().source_name, "First");
}

#[test]
fn unresolved_references_report_each_use_in_depth_first_order() {
    let mut foreign_builder = ApiBuilder::new();
    foreign_builder.reserve_definition();
    foreign_builder.reserve_definition();
    let out_of_range = foreign_builder.reserve_definition();

    let mut builder = ApiBuilder::new();
    let owner = builder.reserve_definition();
    let missing = builder.reserve_definition();
    builder
        .define(
            owner,
            definition(
                "Owner",
                SchemaUse::new(TypeExpr::Object(ObjectSchema {
                    properties: vec![
                        Property {
                            wire_name: "items".into(),
                            required: true,
                            value: SchemaUse::new(TypeExpr::Array(ArraySchema {
                                items: Box::new(reference(out_of_range, "/items/items")),
                                constraints: ArrayConstraints::default(),
                            })),
                            policy: PropertyPolicy::default(),
                        },
                        Property {
                            wire_name: "value".into(),
                            required: true,
                            value: reference(out_of_range, "/value"),
                            policy: PropertyPolicy::default(),
                        },
                    ],
                    additional_properties: AdditionalProperties::Schema(Box::new(reference(
                        out_of_range,
                        "/additionalProperties",
                    ))),
                })),
            ),
        )
        .unwrap();

    let errors = builder.finish().unwrap_err();
    assert_eq!(
        errors.errors(),
        &[
            BuildError::UnresolvedReference {
                owner: GraphOwner::Definition(owner),
                target: out_of_range,
                location: Some(SourceRef {
                    document: "graph.json".into(),
                    pointer: "/items/items".into(),
                }),
            },
            BuildError::UnresolvedReference {
                owner: GraphOwner::Definition(owner),
                target: out_of_range,
                location: Some(SourceRef {
                    document: "graph.json".into(),
                    pointer: "/value".into(),
                }),
            },
            BuildError::UnresolvedReference {
                owner: GraphOwner::Definition(owner),
                target: out_of_range,
                location: Some(SourceRef {
                    document: "graph.json".into(),
                    pointer: "/additionalProperties".into(),
                }),
            },
            BuildError::MissingDefinition { id: missing },
        ]
    );

    let mut valid_builder = ApiBuilder::new();
    valid_builder.add_definition(empty_object("Valid"));
    let valid_api = valid_builder.finish().unwrap();
    assert_eq!(valid_api.definition(out_of_range), None);
}

#[test]
fn empty_builder_finalizes() {
    let api = ApiBuilder::new().finish().unwrap();
    assert_eq!(api.definitions().count(), 0);
}

/// Nested compositions retain kind, order, mappings, and sources after
/// finalization and `Api::clone`.
#[test]
fn nested_compositions_retain_structure_after_finish_and_clone() {
    let mut builder = ApiBuilder::new();
    let shared = builder.reserve_definition();
    let nested_owner = builder.reserve_definition();

    shared_definition(&mut builder, shared);
    builder
        .define(nested_owner, nested_union_definition(shared))
        .unwrap();

    let api = builder.finish().unwrap();
    let cloned = api.clone();

    for graph in [&api, &cloned] {
        let schema = &graph.definition(nested_owner).unwrap().schema;
        let TypeExpr::Composition(outer) = &schema.ty else {
            panic!("Nested should be a composition");
        };
        assert_eq!(outer.kind, CompositionKind::AllOf);
        assert_eq!(outer.branches.len(), 2);
        assert!(matches!(outer.branches[0].ty, TypeExpr::Ref(id) if id == shared));
        assert_eq!(
            outer.branches[0]
                .annotations
                .source
                .as_ref()
                .unwrap()
                .pointer,
            "/outer/branch-0"
        );

        let TypeExpr::Composition(nested) = &outer.branches[1].ty else {
            panic!("Outer branch 1 should be a nested composition");
        };
        assert_eq!(nested.kind, CompositionKind::AnyOf);
        assert!(matches!(nested.branches[0].ty, TypeExpr::Ref(id) if id == shared));
        assert!(nested.branches[0].nullable);
        assert_eq!(
            nested.branches[0]
                .annotations
                .source
                .as_ref()
                .unwrap()
                .pointer,
            "/nested/branch-0"
        );
        assert!(matches!(nested.branches[1].ty, TypeExpr::Null));

        let discriminator = outer.discriminator.as_ref().unwrap();
        assert_eq!(discriminator.property_name, "kind");
        assert_eq!(discriminator.mappings.len(), 2);
        assert_eq!(discriminator.mappings[0].wire_value, "first");
        assert!(matches!(discriminator.mappings[0].target, id if id == shared));
        assert_eq!(
            discriminator.mappings[0].source.as_ref().unwrap().pointer,
            "/outer/mapping-0"
        );
        assert_eq!(discriminator.mappings[1].wire_value, "second");
        assert!(discriminator.mappings[1].source.is_none());
    }

    assert_eq!(direct_references(&api, nested_owner), [shared; 4]);
}

/// Defines the boolean target shared across composition branches.
fn shared_definition(builder: &mut ApiBuilder, shared: DefinitionId) {
    builder
        .define(
            shared,
            definition(
                "Shared",
                SchemaUse {
                    ty: TypeExpr::Boolean,
                    nullable: false,
                    annotations: SchemaAnnotations {
                        source: Some(SourceRef {
                            document: "graph.json".into(),
                            pointer: "/$defs/Shared".into(),
                        }),
                        ..SchemaAnnotations::default()
                    },
                },
            ),
        )
        .unwrap();
}

/// Defines the nested `AllOf` composition with discriminator mappings.
fn nested_union_definition(shared: DefinitionId) -> Definition {
    let nested_branch = SchemaUse {
        ty: TypeExpr::Composition(CompositionSchema {
            kind: CompositionKind::AnyOf,
            branches: vec![
                SchemaUse {
                    ty: TypeExpr::Ref(shared),
                    nullable: true,
                    annotations: SchemaAnnotations {
                        source: Some(SourceRef {
                            document: "graph.json".into(),
                            pointer: "/nested/branch-0".into(),
                        }),
                        ..SchemaAnnotations::default()
                    },
                },
                SchemaUse::new(TypeExpr::Null),
            ],
            discriminator: None,
        }),
        nullable: false,
        annotations: SchemaAnnotations::default(),
    };
    definition(
        "Nested",
        SchemaUse::new(TypeExpr::Composition(CompositionSchema {
            kind: CompositionKind::AllOf,
            branches: vec![
                SchemaUse {
                    ty: TypeExpr::Ref(shared),
                    nullable: false,
                    annotations: SchemaAnnotations {
                        source: Some(SourceRef {
                            document: "graph.json".into(),
                            pointer: "/outer/branch-0".into(),
                        }),
                        ..SchemaAnnotations::default()
                    },
                },
                nested_branch,
            ],
            discriminator: Some(Discriminator {
                property_name: "kind".into(),
                mappings: vec![
                    DiscriminatorMapping {
                        wire_value: "first".into(),
                        target: shared,
                        source: Some(SourceRef {
                            document: "graph.json".into(),
                            pointer: "/outer/mapping-0".into(),
                        }),
                    },
                    DiscriminatorMapping {
                        wire_value: "second".into(),
                        target: shared,
                        source: None,
                    },
                ],
            }),
        })),
    )
}
