//! Exercises the completed semantic IR contract as a public-API consumer.
//!
//! Builds a shared coordinate interpretation with distinct property-local
//! policies, a nested composition with a resolved discriminator mapping, and
//! an HTTP response keeping the original envelope and a projected output.

use satay_ir::{
    AdditionalProperties, ApiBuilder, ArrayConstraints, ArraySchema, CompositionKind,
    CompositionSchema, CoordinatesInterpretation, DecodePolicy, Definition, DefinitionId,
    Discriminator, DiscriminatorMapping, HttpApi, HttpMethod, MediaType, ObjectSchema, Operation,
    OperationInterpretation, OutputSelector, Parameter, ParameterLocation, PathItem, Property,
    PropertyPolicy, Response, ResponseMediaType, ResponseProjection, ResponseStatus,
    SchemaAnnotations, SchemaUse, SecurityRequirement, SecurityScheme, SourceRef,
    StringInterpretation, StringSchema, TypeExpr,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ApiBuilder::new();

    // A logical two-number-field target and the shared coordinate schema.
    let placement = builder.reserve_definition();
    let coordinates = builder.reserve_definition();
    let report = builder.reserve_definition();
    let union = builder.reserve_definition();

    build_definitions(&mut builder, placement, coordinates, report, union)?;
    let envelope = build_envelope_and_http(&mut builder, coordinates);
    let api = builder.finish()?;
    verify(&api, placement, coordinates, report, union, envelope)?;

    println!("coordinate uses: shared definition, independent policies");
    println!("composition: ordered branches and resolved mappings");
    println!("response: original envelope and projected items retained");

    Ok(())
}
/// Defines placement, the shared coordinates schema, report policies, and the
/// nested composition.
fn build_definitions(
    builder: &mut ApiBuilder,
    placement: DefinitionId,
    coordinates: DefinitionId,
    report: DefinitionId,
    union: DefinitionId,
) -> Result<(), Box<dyn std::error::Error>> {
    build_shape_definitions(builder, placement, coordinates)?;
    build_policy_definitions(builder, coordinates, report, union)?;
    Ok(())
}

/// Defines the packed-coordinate target and the shared coordinate schema.
fn build_shape_definitions(
    builder: &mut ApiBuilder,
    placement: DefinitionId,
    coordinates: DefinitionId,
) -> Result<(), Box<dyn std::error::Error>> {
    builder.define(
        placement,
        Definition {
            source_name: "Placement".into(),
            schema: SchemaUse::new(TypeExpr::Object(ObjectSchema {
                properties: vec![
                    Property {
                        wire_name: "latitude".into(),
                        required: true,
                        value: SchemaUse::new(TypeExpr::AnyJson),
                        policy: PropertyPolicy::default(),
                    },
                    Property {
                        wire_name: "longitude".into(),
                        required: true,
                        value: SchemaUse::new(TypeExpr::AnyJson),
                        policy: PropertyPolicy::default(),
                    },
                ],
                additional_properties: AdditionalProperties::Forbidden,
            })),
        },
    )?;

    builder.define(
        coordinates,
        Definition {
            source_name: "Coordinates".into(),
            schema: SchemaUse {
                ty: TypeExpr::String(StringSchema {
                    interpretation: StringInterpretation::Coordinates(
                        CoordinatesInterpretation::new(
                            placement,
                            ["latitude".into(), "longitude".into()],
                            " ".into(),
                        )?,
                    ),
                    ..StringSchema::default()
                }),
                nullable: false,
                annotations: SchemaAnnotations {
                    source: Some(SourceRef {
                        document: "contract.json".into(),
                        pointer: "/components/Coordinates".into(),
                    }),
                    ..SchemaAnnotations::default()
                },
            },
        },
    )?;

    Ok(())
}

/// Defines the report object and the nested composition under local policies.
fn build_policy_definitions(
    builder: &mut ApiBuilder,
    coordinates: DefinitionId,
    report: DefinitionId,
    union: DefinitionId,
) -> Result<(), Box<dyn std::error::Error>> {
    // Two properties share the coordinate definition under different failure
    // policies; the shared schema itself is not mutated by either.
    builder.define(
        report,
        Definition {
            source_name: "Report".into(),
            schema: SchemaUse::new(TypeExpr::Object(ObjectSchema {
                properties: vec![
                    Property {
                        wire_name: "source".into(),
                        required: true,
                        value: SchemaUse::new(TypeExpr::Ref(coordinates)),
                        policy: PropertyPolicy::Included {
                            identifier: None,
                            decoding: DecodePolicy::ErrorAsAbsent,
                        },
                    },
                    Property {
                        wire_name: "destination".into(),
                        required: false,
                        value: SchemaUse::new(TypeExpr::Ref(coordinates)),
                        policy: PropertyPolicy::Included {
                            identifier: Some(vec!["destination".into(), "location".into()]),
                            decoding: DecodePolicy::PropagateError,
                        },
                    },
                ],
                additional_properties: AdditionalProperties::Forbidden,
            })),
        },
    )?;

    builder.define(
        union,
        Definition {
            source_name: "Union".into(),
            schema: SchemaUse::new(TypeExpr::Composition(CompositionSchema {
                kind: CompositionKind::OneOf,
                branches: vec![SchemaUse::new(TypeExpr::Composition(CompositionSchema {
                    kind: CompositionKind::AnyOf,
                    branches: vec![
                        SchemaUse {
                            ty: TypeExpr::Ref(report),
                            nullable: false,
                            annotations: SchemaAnnotations {
                                source: Some(SourceRef {
                                    document: "contract.json".into(),
                                    pointer: "/components/Union/nested/branches/0".into(),
                                }),
                                ..SchemaAnnotations::default()
                            },
                        },
                        SchemaUse::new(TypeExpr::Null),
                    ],
                    discriminator: None,
                }))],
                discriminator: Some(Discriminator {
                    property_name: "kind".into(),
                    mappings: vec![DiscriminatorMapping {
                        wire_value: "report".into(),
                        target: report,
                        source: Some(SourceRef {
                            document: "contract.json".into(),
                            pointer: "/components/Union/mappings/0".into(),
                        }),
                    }],
                }),
            })),
        },
    )?;

    Ok(())
}

/// Adds the envelope definition and the HTTP record, returning the envelope ID.
fn build_envelope_and_http(builder: &mut ApiBuilder, coordinates: DefinitionId) -> DefinitionId {
    let envelope = builder.add_definition(Definition {
        source_name: "Envelope".into(),
        schema: SchemaUse::new(TypeExpr::Object(ObjectSchema {
            properties: vec![Property {
                wire_name: "items".into(),
                required: false,
                value: SchemaUse::new(TypeExpr::Array(ArraySchema {
                    items: Box::new(SchemaUse::new(TypeExpr::Object(ObjectSchema {
                        properties: vec![Property {
                            wire_name: "value".into(),
                            required: false,
                            value: SchemaUse::new(TypeExpr::Ref(coordinates)),
                            policy: PropertyPolicy::default(),
                        }],
                        additional_properties: AdditionalProperties::Unspecified,
                    }))),
                    constraints: ArrayConstraints::default(),
                })),
                policy: PropertyPolicy::default(),
            }],
            additional_properties: AdditionalProperties::Unspecified,
        })),
    });

    let mut http = HttpApi::default();
    http.paths.push(PathItem {
        path: "/reports".into(),
        parameters: vec![Parameter {
            wire_name: "report_id".into(),
            location: ParameterLocation::Path,
            required: true,
            description: None,
            schema: SchemaUse::new(TypeExpr::String(StringSchema::default())),
            style: None,
            explode: None,
            allow_reserved: None,
            allow_empty_value: None,
            source: None,
        }],
        operations: vec![Operation {
            source_id: None,
            method: HttpMethod::Get,
            description: None,
            tags: Vec::new(),
            parameters: Vec::new(),
            request_body: None,
            responses: vec![Response {
                status: ResponseStatus::Exact(200),
                description: None,
                content: vec![ResponseMediaType {
                    media: MediaType {
                        media_type: "application/json".into(),
                        schema: Some(SchemaUse::new(TypeExpr::Ref(envelope))),
                        source: None,
                    },
                    projection: Some(ResponseProjection {
                        selector: OutputSelector {
                            unwrap_field: "items".into(),
                            map_field: Some("value".into()),
                        },
                        output: SchemaUse::new(TypeExpr::Array(ArraySchema {
                            items: Box::new(SchemaUse {
                                ty: TypeExpr::Ref(coordinates),
                                nullable: true,
                                annotations: SchemaAnnotations::default(),
                            }),
                            constraints: ArrayConstraints::default(),
                        })),
                    }),
                }],
                source: None,
            }],
            servers: None,
            security: Some(vec![SecurityRequirement {
                schemes: Vec::new(),
            }]),
            interpretation: OperationInterpretation::default(),
            source: None,
        }],
        servers: None,
        source: None,
    });
    http.security_schemes.push(SecurityScheme {
        name: "anonymous".into(),
        description: None,
        kind: satay_ir::SecuritySchemeKind::MutualTls,
    });
    builder.set_http(http);
    envelope
}

fn verify(
    api: &satay_ir::Api,
    placement: DefinitionId,
    coordinates: DefinitionId,
    report: DefinitionId,
    union: DefinitionId,
    envelope: DefinitionId,
) -> Result<(), Box<dyn std::error::Error>> {
    verify_coordinates(api, placement, coordinates, report)?;
    verify_composition(api, union, report)?;
    verify_response(api, envelope, coordinates)?;
    Ok(())
}

/// Verifies the two coordinate properties and the shared schema.
fn verify_coordinates(
    api: &satay_ir::Api,
    placement: DefinitionId,
    coordinates: DefinitionId,
    report: DefinitionId,
) -> Result<(), Box<dyn std::error::Error>> {
    // Coordinate uses: shared definition, independent policies.
    let TypeExpr::Object(report_schema) =
        &api.definition(report).ok_or("Report is missing")?.schema.ty
    else {
        return Err("Report is not an object".into());
    };
    let source_policy = &report_schema.properties[0].policy;
    let destination_policy = &report_schema.properties[1].policy;
    let PropertyPolicy::Included {
        identifier: source_identifier,
        decoding: source_decoding,
    } = source_policy
    else {
        return Err("property policies do not match the contract".into());
    };
    let PropertyPolicy::Included {
        identifier: destination_identifier,
        decoding: destination_decoding,
    } = destination_policy
    else {
        return Err("property policies do not match the contract".into());
    };
    if source_identifier.is_some() || destination_identifier.is_none() {
        return Err("property policies do not match the contract".into());
    }
    if !matches!(source_decoding, DecodePolicy::ErrorAsAbsent)
        || !matches!(destination_decoding, DecodePolicy::PropagateError)
    {
        return Err("decoding policies do not match the contract".into());
    }
    if !matches!(
        report_schema.properties[0].value.ty,
        TypeExpr::Ref(id) if id == coordinates
    ) || !matches!(
        report_schema.properties[1].value.ty,
        TypeExpr::Ref(id) if id == coordinates
    ) {
        return Err("properties do not reference the shared coordinate definition".into());
    }
    let TypeExpr::String(shared_string) = &api
        .definition(coordinates)
        .ok_or("Coordinates is missing")?
        .schema
        .ty
    else {
        return Err("Coordinates is not a string".into());
    };
    let StringInterpretation::Coordinates(shared_coordinates) = &shared_string.interpretation
    else {
        return Err("Coordinates carries no coordinates interpretation".into());
    };
    if shared_coordinates.target() != placement
        || shared_coordinates.fields() != &["latitude".to_string(), "longitude".to_string()]
        || shared_coordinates.delimiter() != " "
    {
        return Err("shared coordinate schema was mutated".into());
    }

    Ok(())
}

/// Verifies the nested composition and its resolved mapping.
fn verify_composition(
    api: &satay_ir::Api,
    union: DefinitionId,
    report: DefinitionId,
) -> Result<(), Box<dyn std::error::Error>> {
    // Composition: ordered branches and resolved mappings.
    let TypeExpr::Composition(union_schema) =
        &api.definition(union).ok_or("Union is missing")?.schema.ty
    else {
        return Err("Union is not a composition".into());
    };
    if union_schema.kind != CompositionKind::OneOf || union_schema.branches.len() != 1 {
        return Err("outer composition kind or branch count does not match the contract".into());
    }
    let TypeExpr::Composition(nested_schema) = &union_schema.branches[0].ty else {
        return Err("outer composition branch 0 is not a nested composition".into());
    };
    if nested_schema.kind != CompositionKind::AnyOf
        || nested_schema.branches.len() != 2
        || !matches!(nested_schema.branches[0].ty, TypeExpr::Ref(id) if id == report)
        || !matches!(nested_schema.branches[1].ty, TypeExpr::Null)
    {
        return Err("nested composition branches do not match the contract".into());
    }
    if nested_schema.branches[0]
        .annotations
        .source
        .as_ref()
        .map(|s| s.pointer.as_str())
        != Some("/components/Union/nested/branches/0")
    {
        return Err("nested branch source was not retained".into());
    }
    let mapping = &union_schema
        .discriminator
        .as_ref()
        .ok_or("Union carries no discriminator")?
        .mappings[0];
    if mapping.wire_value != "report" || mapping.target != report {
        return Err("discriminator mapping is not resolved as declared".into());
    }

    Ok(())
}

/// Verifies the original envelope and projected response items.
fn verify_response(
    api: &satay_ir::Api,
    envelope: DefinitionId,
    coordinates: DefinitionId,
) -> Result<(), Box<dyn std::error::Error>> {
    // Response: original envelope and projected items retained.
    let response = &api.http().paths[0].operations[0].responses[0];
    let media = &response.content[0];
    let TypeExpr::Ref(original_id) = &media
        .media
        .schema
        .as_ref()
        .ok_or("original schema missing")?
        .ty
    else {
        return Err("original media schema is not a reference".into());
    };
    if *original_id != envelope {
        return Err("original media schema does not reference Envelope".into());
    }
    let TypeExpr::Object(envelope_schema) = &api
        .definition(envelope)
        .ok_or("Envelope is missing")?
        .schema
        .ty
    else {
        return Err("Envelope is not an object".into());
    };
    let items = &envelope_schema.properties[0];
    if items.required {
        return Err("Envelope.items was normalized to required".into());
    }
    let TypeExpr::Array(items_array) = &items.value.ty else {
        return Err("Envelope.items is not an array".into());
    };
    let TypeExpr::Object(item_object) = &items_array.items.ty else {
        return Err("Envelope.items is not an array of objects".into());
    };
    let value = &item_object.properties[0];
    if value.required {
        return Err("Envelope.items.value was normalized".into());
    }
    if !matches!(value.value.ty, TypeExpr::Ref(id) if id == coordinates) {
        return Err("Envelope.items.value does not reference Coordinates".into());
    }
    let projection = &media.projection.as_ref().ok_or("projection is missing")?;
    let TypeExpr::Array(projected_array) = &projection.output.ty else {
        return Err("projection output is not an array".into());
    };
    if !projected_array.items.nullable
        || !matches!(projected_array.items.ty, TypeExpr::Ref(id) if id == coordinates)
    {
        return Err("projected items are not nullable shared coordinates".into());
    }

    Ok(())
}
