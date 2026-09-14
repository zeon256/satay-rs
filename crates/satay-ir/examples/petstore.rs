//! Builds and inspects a small schema graph with forward references.

use std::io;

use satay_ir::{
    AdditionalProperties, Api, ApiBuilder, ArrayConstraints, ArraySchema, Definition, DefinitionId,
    ObjectSchema, Property, SchemaUse, StringSchema, TypeExpr,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ApiBuilder::new();
    let user = builder.reserve_definition();
    let pet = builder.reserve_definition();
    let pet_list = builder.reserve_definition();

    build_graph(&mut builder, user, pet, pet_list)?;

    let api = builder.finish()?;
    let (names, owner_name, item_name) = verify(&api, user, pet, pet_list)?;

    println!("definitions: {}", names.join(", "));
    println!("Pet.owner -> {owner_name}");
    println!("PetList.items -> {item_name}");

    Ok(())
}

/// Allocates the three definitions in caller order with forward references.
fn build_graph(
    builder: &mut ApiBuilder,
    user: DefinitionId,
    pet: DefinitionId,
    pet_list: DefinitionId,
) -> Result<(), io::Error> {
    builder
        .define(
            pet_list,
            Definition {
                source_name: "PetList".into(),
                schema: SchemaUse::new(TypeExpr::Array(ArraySchema {
                    items: Box::new(SchemaUse::new(TypeExpr::Ref(pet))),
                    constraints: ArrayConstraints::default(),
                })),
            },
        )
        .map_err(|_| graph_error("PetList define failed"))?;
    builder
        .define(
            pet,
            Definition {
                source_name: "Pet".into(),
                schema: SchemaUse::new(TypeExpr::Object(ObjectSchema {
                    properties: vec![
                        Property {
                            wire_name: "owner".into(),
                            required: true,
                            value: SchemaUse::new(TypeExpr::Ref(user)),
                        },
                        Property {
                            wire_name: "reviewer".into(),
                            required: false,
                            value: SchemaUse::new(TypeExpr::Ref(user)),
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
                        },
                    ],
                    additional_properties: AdditionalProperties::Forbidden,
                })),
            },
        )
        .map_err(|_| graph_error("Pet define failed"))?;
    builder
        .define(
            user,
            Definition {
                source_name: "User".into(),
                schema: SchemaUse::new(TypeExpr::Object(ObjectSchema {
                    properties: Vec::new(),
                    additional_properties: AdditionalProperties::Unspecified,
                })),
            },
        )
        .map_err(|_| graph_error("User define failed"))?;

    Ok(())
}

/// Reads the finalized graph and returns names plus the linked target names.
fn verify(
    api: &Api,
    user: DefinitionId,
    pet: DefinitionId,
    pet_list: DefinitionId,
) -> Result<(Vec<String>, String, String), io::Error> {
    let names = api
        .definitions()
        .map(|(_, definition)| definition.source_name.clone())
        .collect::<Vec<_>>();
    if names != ["User".to_string(), "Pet".to_string(), "PetList".to_string()] {
        return Err(graph_error("definition allocation order changed"));
    }

    let pet_definition = api
        .definition(pet)
        .ok_or_else(|| graph_error("Pet definition is missing"))?;
    let TypeExpr::Object(pet_schema) = &pet_definition.schema.ty else {
        return Err(graph_error("Pet is not an object"));
    };
    let owner = pet_schema
        .properties
        .iter()
        .find(|property| property.wire_name == "owner")
        .ok_or_else(|| graph_error("Pet.owner is missing"))?;
    let TypeExpr::Ref(owner_id) = owner.value.ty else {
        return Err(graph_error("Pet.owner is not a reference"));
    };
    if owner_id != user {
        return Err(graph_error("Pet.owner does not reference User"));
    }
    let owner_name = api
        .definition(owner_id)
        .ok_or_else(|| graph_error("Pet.owner target is missing"))?
        .source_name
        .clone();

    let tags = pet_schema
        .properties
        .iter()
        .find(|property| property.wire_name == "tags")
        .ok_or_else(|| graph_error("Pet.tags is missing"))?;
    let TypeExpr::Array(tags_schema) = &tags.value.ty else {
        return Err(graph_error("Pet.tags is not an array"));
    };
    if !matches!(tags_schema.items.ty, TypeExpr::String(_)) {
        return Err(graph_error("Pet.tags items are not inline strings"));
    }

    let list_definition = api
        .definition(pet_list)
        .ok_or_else(|| graph_error("PetList definition is missing"))?;
    let TypeExpr::Array(list_schema) = &list_definition.schema.ty else {
        return Err(graph_error("PetList is not an array"));
    };
    let TypeExpr::Ref(item_id) = list_schema.items.ty else {
        return Err(graph_error("PetList items are not a reference"));
    };
    if item_id != pet {
        return Err(graph_error("PetList items do not reference Pet"));
    }
    let item_name = api
        .definition(item_id)
        .ok_or_else(|| graph_error("PetList item target is missing"))?
        .source_name
        .clone();

    Ok((names, owner_name, item_name))
}

fn graph_error(message: &'static str) -> io::Error {
    io::Error::other(message)
}
