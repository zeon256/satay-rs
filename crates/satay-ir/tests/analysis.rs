//! Consumer-side fixed-point analysis over the finalized graph.
//!
//! This is a graph-consumer capability demonstration, not a production
//! storage analysis and not a field on the IR.

use std::collections::HashMap;

use satay_ir::{
    AdditionalProperties, ArraySchema, CompositionKind, CompositionSchema,
    CoordinatesInterpretation, Definition, DefinitionId, ObjectSchema, Property, PropertyPolicy,
    SchemaUse, StringInterpretation, StringSchema, TypeExpr,
};

/// Consumer-local summary flags.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Requirements {
    text: bool,
    collection: bool,
}

fn definition(source_name: &str, schema: SchemaUse) -> Definition {
    Definition {
        source_name: source_name.into(),
        schema,
    }
}

/// Collects local requirements and outgoing edges for one definition.
fn analyze(schema: &SchemaUse, requirements: &mut Requirements, outgoing: &mut Vec<DefinitionId>) {
    match &schema.ty {
        TypeExpr::Ref(target) => outgoing.push(*target),
        TypeExpr::String(string_schema) => {
            requirements.text = true;
            if let StringInterpretation::Coordinates(coordinates) = &string_schema.interpretation {
                outgoing.push(coordinates.target());
            }
        }
        TypeExpr::Array(array) => {
            requirements.collection = true;
            analyze(&array.items, requirements, outgoing);
        }
        TypeExpr::Object(object) => {
            if matches!(
                object.additional_properties,
                AdditionalProperties::Allowed | AdditionalProperties::Schema(_)
            ) {
                requirements.collection = true;
            }
            for property in &object.properties {
                analyze(&property.value, requirements, outgoing);
            }
            if let AdditionalProperties::Schema(value) = &object.additional_properties {
                analyze(value, requirements, outgoing);
            }
        }
        TypeExpr::Composition(composition) => {
            for branch in &composition.branches {
                analyze(branch, requirements, outgoing);
            }
            if let Some(discriminator) = &composition.discriminator {
                for mapping in &discriminator.mappings {
                    outgoing.push(mapping.target);
                }
            }
        }
        _ => {}
    }
}

fn local_requirements(schema: &SchemaUse) -> (Requirements, Vec<DefinitionId>) {
    let mut requirements = Requirements::default();
    let mut outgoing = vec![];
    analyze(schema, &mut requirements, &mut outgoing);
    (requirements, outgoing)
}

/// Fixed-point OR-propagation from local requirements along edges.
fn solve(api: &satay_ir::Api) -> HashMap<DefinitionId, Requirements> {
    let mut state = api
        .definitions()
        .map(|(id, definition)| (id, local_requirements(&definition.schema)))
        .collect::<HashMap<DefinitionId, (Requirements, Vec<DefinitionId>)>>();

    let mut changed = true;
    let mut rounds = 0;
    while changed {
        changed = false;
        rounds += 1;
        // Snapshot edges so one definition can be read while another updates.
        let edges = state
            .iter()
            .map(|(owner, (_, outgoing))| (*owner, outgoing.clone()))
            .collect::<Vec<(DefinitionId, Vec<DefinitionId>)>>();
        for (owner, outgoing) in edges {
            for target in outgoing {
                let referenced = state[&target].0;
                let requirements = &mut state.get_mut(&owner).unwrap().0;
                if referenced.text && !requirements.text {
                    requirements.text = true;
                    changed = true;
                }
                if referenced.collection && !requirements.collection {
                    requirements.collection = true;
                    changed = true;
                }
            }
        }
    }

    assert!(rounds < 10, "propagation must terminate");
    state
        .into_iter()
        .map(|(id, (requirements, _))| (id, requirements))
        .collect()
}

/// Builds the A/B/C/D cycle-and-isolation core.
fn build_cycle(builder: &mut satay_ir::ApiBuilder, ids: [DefinitionId; 4]) {
    let [text_owner, array_owner, union_owner, isolated] = ids;

    builder
        .define(
            text_owner,
            definition(
                "A",
                SchemaUse::new(TypeExpr::Object(ObjectSchema {
                    properties: vec![
                        Property {
                            wire_name: "label".into(),
                            required: true,
                            value: SchemaUse::new(TypeExpr::String(StringSchema::default())),
                            policy: PropertyPolicy::default(),
                        },
                        Property {
                            wire_name: "peer".into(),
                            required: true,
                            value: SchemaUse::new(TypeExpr::Ref(array_owner)),
                            policy: PropertyPolicy::default(),
                        },
                    ],
                    additional_properties: AdditionalProperties::Unspecified,
                })),
            ),
        )
        .unwrap();
    builder
        .define(
            array_owner,
            definition(
                "B",
                SchemaUse::new(TypeExpr::Array(ArraySchema {
                    items: Box::new(SchemaUse::new(TypeExpr::Ref(text_owner))),
                    constraints: satay_ir::ArrayConstraints::default(),
                })),
            ),
        )
        .unwrap();
    builder
        .define(
            union_owner,
            definition(
                "C",
                SchemaUse::new(TypeExpr::Composition(CompositionSchema {
                    kind: CompositionKind::AnyOf,
                    branches: vec![SchemaUse::new(TypeExpr::Ref(text_owner))],
                    discriminator: None,
                })),
            ),
        )
        .unwrap();
    builder
        .define(isolated, definition("D", SchemaUse::new(TypeExpr::Boolean)))
        .unwrap();
}

/// Builds the string definitions carrying coordinate edges into `union_owner`
/// and returns their IDs.
fn build_string_uses(
    builder: &mut satay_ir::ApiBuilder,
    union_owner: DefinitionId,
) -> [DefinitionId; 2] {
    let root_string = builder.reserve_definition();
    let nested_string = builder.reserve_definition();

    builder
        .define(
            root_string,
            definition(
                "S",
                SchemaUse::new(TypeExpr::String(StringSchema {
                    interpretation: StringInterpretation::Coordinates(
                        CoordinatesInterpretation::new(
                            union_owner,
                            ["latitude".into(), "longitude".into()],
                            " ".into(),
                        )
                        .unwrap(),
                    ),
                    ..StringSchema::default()
                })),
            ),
        )
        .unwrap();
    builder
        .define(
            nested_string,
            definition(
                "T",
                SchemaUse::new(TypeExpr::Object(ObjectSchema {
                    properties: vec![Property {
                        wire_name: "position".into(),
                        required: true,
                        // A nested (non-root) coordinate edge: only discovered
                        // when the recursive analysis descends into properties.
                        value: SchemaUse::new(TypeExpr::String(StringSchema {
                            interpretation: StringInterpretation::Coordinates(
                                CoordinatesInterpretation::new(
                                    union_owner,
                                    ["longitude".into(), "latitude".into()],
                                    ",".into(),
                                )
                                .unwrap(),
                            ),
                            ..StringSchema::default()
                        })),
                        policy: PropertyPolicy::default(),
                    }],
                    additional_properties: AdditionalProperties::Unspecified,
                })),
            ),
        )
        .unwrap();

    [root_string, nested_string]
}

#[test]
fn text_and_collection_requirements_reach_fixed_point() {
    let mut builder = satay_ir::ApiBuilder::new();
    let text_owner = builder.reserve_definition();
    let array_owner = builder.reserve_definition();
    let union_owner = builder.reserve_definition();
    let isolated = builder.reserve_definition();
    build_cycle(
        &mut builder,
        [text_owner, array_owner, union_owner, isolated],
    );
    let [root_string, nested_string] = build_string_uses(&mut builder, union_owner);

    let api = builder.finish().unwrap();
    let solved = solve(&api);

    assert!(solved[&text_owner].text);
    assert!(solved[&text_owner].collection);
    assert!(solved[&array_owner].text);
    assert!(solved[&array_owner].collection);
    assert!(solved[&union_owner].text);
    assert!(solved[&union_owner].collection);
    assert!(!solved[&isolated].text);
    assert!(!solved[&isolated].collection);
    // The root string's coordinate edge to C propagates C's requirements in.
    assert!(solved[&root_string].text);
    assert!(solved[&root_string].collection);
    // T's inline coordinate edge reaches C through the recursive walk.
    assert!(solved[&nested_string].text);
    assert!(solved[&nested_string].collection);
}
