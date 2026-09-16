//! Structural and retention checks over owned HTTP roots.
use satay_ir::{
    AdditionalProperties, ApiBuilder, ApiKeyLocation, ArrayConstraints, ArraySchema, BuildError,
    CompositionKind, CompositionSchema, CoordinatesInterpretation, Definition, DefinitionId,
    Discriminator, DiscriminatorMapping, GraphOwner, HttpApi, HttpMethod, MediaType, ObjectSchema,
    Operation, OperationInterpretation, OutputSelector, Parameter, ParameterLocation,
    ParameterStyle, PathItem, Property, PropertyPolicy, RequestBody, Response, ResponseMediaType,
    ResponseProjection, ResponseStatus, SchemaAnnotations, SchemaUse, SecurityRequirement,
    SecurityRequirementScheme, SecurityScheme, SecuritySchemeKind, Server, ServerVariable,
    SourceRef, StringInterpretation, StringSchema, Tag, TypeExpr,
};

fn definition(source_name: &str, schema: SchemaUse) -> Definition {
    Definition {
        source_name: source_name.into(),
        schema,
    }
}

fn reference(target: DefinitionId, pointer: &str) -> SchemaUse {
    SchemaUse {
        ty: TypeExpr::Ref(target),
        nullable: false,
        annotations: SchemaAnnotations {
            source: Some(source(pointer)),
            ..SchemaAnnotations::default()
        },
    }
}

fn unlocated(target: DefinitionId) -> SchemaUse {
    SchemaUse {
        ty: TypeExpr::Ref(target),
        nullable: false,
        annotations: SchemaAnnotations::default(),
    }
}

fn source(pointer: &str) -> SourceRef {
    SourceRef {
        document: "api.json".into(),
        pointer: pointer.into(),
    }
}

fn coordinate(
    target: DefinitionId,
    fields: [&str; 2],
    delimiter: &str,
    pointer: &str,
) -> SchemaUse {
    SchemaUse {
        ty: TypeExpr::String(StringSchema {
            interpretation: StringInterpretation::Coordinates(
                CoordinatesInterpretation::new(
                    target,
                    [fields[0].into(), fields[1].into()],
                    delimiter.into(),
                )
                .unwrap(),
            ),
            ..StringSchema::default()
        }),
        nullable: false,
        annotations: SchemaAnnotations {
            source: Some(source(pointer)),
            ..SchemaAnnotations::default()
        },
    }
}

/// One foreign ID exercised through every structural edge kind.
#[test]
fn reports_all_schema_edges_in_graph_order() {
    let out_of_range = foreign_id();
    let mut builder = ApiBuilder::new();
    let missing = builder.reserve_definition();
    let (composition_owner, coordinate_owner) =
        failing_definitions(&mut builder, missing, out_of_range);

    let mut http = HttpApi::default();
    http.paths.push(PathItem {
        path: "/pets".into(),
        parameters: vec![parameter(out_of_range, "/paths/~1pets/parameters/0")],
        operations: vec![operation(out_of_range)],
        servers: None,
        source: None,
    });
    builder.set_http(http);

    let errors = builder.finish().unwrap_err();
    assert_eq!(
        errors.errors(),
        &[
            BuildError::MissingDefinition { id: missing },
            expected_error(
                GraphOwner::Definition(composition_owner),
                out_of_range,
                "/def/Union/branch-a",
            ),
            expected_error(
                GraphOwner::Definition(composition_owner),
                out_of_range,
                "/def/Union/mapping-a",
            ),
            expected_error(
                GraphOwner::Definition(coordinate_owner),
                out_of_range,
                "/def/Placement",
            ),
            expected_error(
                GraphOwner::Path { index: 0 },
                out_of_range,
                "/paths/~1pets/parameters/0",
            ),
            expected_error(
                GraphOwner::Operation {
                    path_index: 0,
                    operation_index: 0,
                },
                out_of_range,
                "/paths/~1pets/get/parameters/0",
            ),
            expected_error(
                GraphOwner::Operation {
                    path_index: 0,
                    operation_index: 0,
                },
                out_of_range,
                "/paths/~1pets/get/requestBody",
            ),
            expected_error(
                GraphOwner::Operation {
                    path_index: 0,
                    operation_index: 0,
                },
                out_of_range,
                "/paths/~1pets/get/responses/200",
            ),
            expected_error(
                GraphOwner::Operation {
                    path_index: 0,
                    operation_index: 0,
                },
                out_of_range,
                "/paths/~1pets/get/responses/200/projection",
            ),
        ],
    );
}

fn parameter(target: DefinitionId, pointer: &str) -> Parameter {
    Parameter {
        wire_name: "pet_id".into(),
        location: ParameterLocation::Path,
        required: true,
        description: None,
        schema: reference(target, pointer),
        style: None,
        explode: None,
        allow_reserved: None,
        allow_empty_value: None,
        source: None,
    }
}

fn operation(target: DefinitionId) -> Operation {
    Operation {
        source_id: None,
        method: HttpMethod::Get,
        description: None,
        tags: vec![],
        parameters: vec![parameter(target, "/paths/~1pets/get/parameters/0")],
        request_body: Some(RequestBody {
            description: None,
            required: true,
            content: vec![MediaType {
                media_type: "application/json".into(),
                schema: Some(reference(target, "/paths/~1pets/get/requestBody")),
                source: None,
            }],
            source: None,
        }),
        responses_diagnostic: None,
        responses: vec![Response {
            status: ResponseStatus::Exact(200),
            description: None,
            content: vec![ResponseMediaType {
                media: MediaType {
                    media_type: "application/json".into(),
                    schema: Some(reference(target, "/paths/~1pets/get/responses/200")),
                    source: None,
                },
                projection: Some(ResponseProjection {
                    unwrap_required: true,
                    map_required: None,
                    selector: OutputSelector {
                        unwrap_field: "items".into(),
                        map_field: None,
                    },
                    output: reference(target, "/paths/~1pets/get/responses/200/projection"),
                }),
            }],
            source: None,
        }],
        servers: None,
        security: None,
        interpretation: OperationInterpretation::default(),
        source: None,
    }
}

/// A cyclic coordinate edge finalizes just like a cyclic reference.
#[test]
fn cyclic_coordinate_edge_finalizes() {
    let mut builder = ApiBuilder::new();
    let self_coordinated = builder.reserve_definition();
    builder
        .define(
            self_coordinated,
            definition(
                "SelfCoord",
                coordinate(
                    self_coordinated,
                    ["latitude", "longitude"],
                    ",",
                    "/def/SelfCoord",
                ),
            ),
        )
        .unwrap();

    let api = builder.finish().unwrap();
    let schema = &api.definition(self_coordinated).unwrap().schema;
    let TypeExpr::String(string_schema) = &schema.ty else {
        panic!("SelfCoord should be a string");
    };
    let StringInterpretation::Coordinates(coordinates) = &string_schema.interpretation else {
        panic!("SelfCoord should carry a coordinates interpretation");
    };
    assert_eq!(coordinates.target(), self_coordinated);
}

/// Security, servers, tags, and parameters are retained without merging.
#[test]
fn retains_security_servers_tags_and_parameter_origins() {
    let mut builder = ApiBuilder::new();
    let bearer = builder.add_definition(definition("Bearer", SchemaUse::new(TypeExpr::AnyJson)));

    builder.set_http(metadata_http(bearer));

    let api = builder.finish().unwrap();
    let http = api.http();

    assert_eq!(http.servers.len(), 1);
    assert_eq!(http.servers[0].variables[0].enum_values.len(), 2);
    assert_eq!(http.security_schemes.len(), 1);
    assert!(matches!(
        &http.security_schemes[0].kind,
        SecuritySchemeKind::ApiKey { wire_name, location }
            if wire_name == "X-Api-Key" && *location == ApiKeyLocation::Header
    ));
    assert_eq!(http.security.len(), 1);

    let path = &http.paths[0];
    assert_eq!(path.servers, Some(vec![]));
    // Path-level and operation-local parameter lists stay separate; same
    // (location, wire_name) is retained with different declarations.
    assert_eq!(path.parameters.len(), 1);
    assert!(path.parameters[0].required);
    assert_eq!(path.parameters[0].explode, Some(false));
    assert_eq!(
        path.parameters[0].description.as_deref(),
        Some("path-level")
    );

    let operation = &path.operations[0];
    assert_eq!(operation.tags, vec!["declared".to_string()]);
    assert_eq!(operation.source_id.as_deref(), Some("declared"));
    assert_eq!(operation.servers, Some(vec![]));
    assert_eq!(
        operation.security,
        Some(vec![SecurityRequirement { schemes: vec![] }])
    );
    assert_eq!(operation.parameters.len(), 1);
    assert!(!operation.parameters[0].required);
    assert_eq!(operation.parameters[0].explode, Some(true));
    assert_eq!(operation.parameters[0].allow_reserved, Some(true));
    assert_eq!(
        operation.parameters[0].description.as_deref(),
        Some("operation-local")
    );
    // Undeclared operation: None retains absence without inference.
    let undeclared = &path.operations[1];
    assert_eq!(undeclared.source_id, None);
    assert_eq!(undeclared.tags, Vec::<String>::new());
    assert_eq!(undeclared.servers, None);
    assert_eq!(undeclared.security, None);
}

/// Response statuses, media entries, and projections are retained verbatim.
#[test]
fn retains_response_media_statuses_and_projection() {
    let mut builder = ApiBuilder::new();
    let pet = builder.add_definition(definition("Pet", SchemaUse::new(TypeExpr::AnyJson)));
    let envelope = builder.add_definition(definition(
        "Envelope",
        SchemaUse::new(TypeExpr::Object(ObjectSchema {
            properties: vec![Property {
                wire_name: "items".into(),
                required: false,
                value: SchemaUse::new(TypeExpr::Array(ArraySchema {
                    items: Box::new(SchemaUse::new(TypeExpr::Object(ObjectSchema {
                        properties: vec![Property {
                            wire_name: "value".into(),
                            required: false,
                            value: SchemaUse::new(TypeExpr::Ref(pet)),
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
    ));

    builder.set_http(projection_http(envelope, pet));

    let api = builder.finish().unwrap();
    let operation = &api.http().paths[0].operations[0];

    let statuses = operation
        .responses
        .iter()
        .map(|response| response.status.clone())
        .collect::<Vec<ResponseStatus>>();
    assert_eq!(
        statuses,
        [
            ResponseStatus::Range(2),
            ResponseStatus::Exact(200),
            ResponseStatus::Default,
            ResponseStatus::Exact(204),
        ]
    );
    // The empty 204 response and a Default status remain representable.
    assert_eq!(operation.responses[1].content.len(), 0);
    assert_eq!(operation.responses[3].content.len(), 0);

    let success = &operation.responses[0];
    assert_eq!(
        success.content[0].media.media_type,
        "application/vnd.example+json"
    );
    assert_eq!(success.content[1].media.media_type, "application/json");

    // The original envelope keeps its optional property and schema shape.
    let envelope_schema = &api.definition(envelope).unwrap().schema;
    let TypeExpr::Object(envelope_object) = &envelope_schema.ty else {
        panic!("Envelope should be an object");
    };
    let items = &envelope_object.properties[0];
    assert!(!items.required);
    let TypeExpr::Array(items_array) = &items.value.ty else {
        panic!("items should be an array");
    };
    let TypeExpr::Object(item_object) = &items_array.items.ty else {
        panic!("items should be an array of objects");
    };
    let value = &item_object.properties[0];
    assert!(!value.required);
    assert!(matches!(value.value.ty, TypeExpr::Ref(id) if id == pet));
    assert!(!value.value.nullable);

    // The projection output is separate from the original schema.
    let projected = &success.content[0].projection.as_ref().unwrap();
    let TypeExpr::Array(projected_array) = &projected.output.ty else {
        panic!("projection output should be an array");
    };
    assert!(projected_array.items.nullable);
    assert!(matches!(projected_array.items.ty, TypeExpr::Ref(id) if id == pet));
    assert_eq!(projected.selector.unwrap_field, "items");
    assert_eq!(projected.selector.map_field.as_deref(), Some("value"));
    // The projection never replaces the original media schema.
    assert!(matches!(
        success.content[0].media.schema.as_ref().unwrap().ty,
        TypeExpr::Ref(id) if id == envelope
    ));
}

/// Reserves foreign slots and returns an ID far beyond this builder.
fn foreign_id() -> DefinitionId {
    let mut foreign_builder = ApiBuilder::new();
    for _ in 0..9 {
        foreign_builder.reserve_definition();
    }
    foreign_builder.reserve_definition()
}

/// Builds the expected unresolved-reference error for one edge.
fn expected_error(owner: GraphOwner, target: DefinitionId, pointer: &str) -> BuildError {
    BuildError::UnresolvedReference {
        owner,
        target,
        location: Some(source(pointer)),
    }
}

/// Builds the root/path/operation metadata fixture for `bearer`.
fn metadata_http(bearer: DefinitionId) -> HttpApi {
    let mut http = HttpApi {
        diagnostic: None,
        servers: vec![Server {
            url: "https://default.example".into(),
            description: Some("default".into()),
            variables: vec![ServerVariable {
                name: "region".into(),
                default: "us".into(),
                enum_values: vec!["us".into(), "eu".into()],
                description: None,
            }],
        }],
        security_schemes: vec![SecurityScheme {
            name: "apiKey".into(),
            description: None,
            kind: SecuritySchemeKind::ApiKey {
                wire_name: "X-Api-Key".into(),
                location: ApiKeyLocation::Header,
            },
        }],
        security: vec![SecurityRequirement {
            schemes: vec![SecurityRequirementScheme {
                scheme: "apiKey".into(),
                scopes: vec![],
            }],
        }],
        tags: vec![Tag {
            name: "declared".into(),
            description: Some("declared root tag".into()),
        }],
        paths: vec![],
    };
    http.paths.push(PathItem {
        path: "/pets".into(),
        parameters: vec![Parameter {
            wire_name: "pet_id".into(),
            location: ParameterLocation::Path,
            required: true,
            description: Some("path-level".into()),
            schema: reference(bearer, "/paths/~1pets/parameters/0"),
            style: Some(ParameterStyle::Simple),
            explode: Some(false),
            allow_reserved: None,
            allow_empty_value: None,
            source: None,
        }],
        operations: vec![
            Operation {
                source_id: Some("declared".into()),
                method: HttpMethod::Get,
                description: None,
                tags: vec!["declared".into()],
                parameters: vec![Parameter {
                    wire_name: "pet_id".into(),
                    location: ParameterLocation::Path,
                    required: false,
                    description: Some("operation-local".into()),
                    schema: reference(bearer, "/paths/~1pets/get/parameters/0"),
                    style: Some(ParameterStyle::Simple),
                    explode: Some(true),
                    allow_reserved: Some(true),
                    allow_empty_value: None,
                    source: None,
                }],
                request_body: None,
                responses_diagnostic: None,
                responses: vec![],
                servers: Some(vec![]),
                security: Some(vec![SecurityRequirement { schemes: vec![] }]),
                interpretation: OperationInterpretation {
                    skip: false,
                    output: None,
                },
                source: None,
            },
            Operation {
                source_id: None,
                method: HttpMethod::Get,
                description: None,
                tags: vec![],
                parameters: vec![],
                request_body: None,
                responses_diagnostic: None,
                responses: vec![],
                servers: None,
                security: None,
                interpretation: OperationInterpretation::default(),
                source: None,
            },
        ],
        servers: Some(vec![]),
        source: None,
    });
    http
}

/// Builds the response media/status/projection fixture.
fn projection_http(envelope: DefinitionId, pet: DefinitionId) -> HttpApi {
    let mut http = HttpApi::default();
    http.paths.push(PathItem {
        path: "/pets".into(),
        parameters: vec![],
        operations: vec![Operation {
            source_id: None,
            method: HttpMethod::Get,
            description: None,
            tags: vec![],
            parameters: vec![],
            request_body: None,
            responses_diagnostic: None,
            responses: vec![
                Response {
                    status: ResponseStatus::Range(2),
                    description: None,
                    content: vec![
                        ResponseMediaType {
                            media: MediaType {
                                media_type: "application/vnd.example+json".into(),
                                schema: Some(reference(envelope, "/envelope")),
                                source: None,
                            },
                            projection: Some(ResponseProjection {
                                unwrap_required: false,
                                map_required: Some(false),
                                selector: OutputSelector {
                                    unwrap_field: "items".into(),
                                    map_field: Some("value".into()),
                                },
                                output: SchemaUse::new(TypeExpr::Array(ArraySchema {
                                    items: Box::new(SchemaUse {
                                        ty: TypeExpr::Ref(pet),
                                        nullable: true,
                                        annotations: SchemaAnnotations::default(),
                                    }),
                                    constraints: ArrayConstraints::default(),
                                })),
                            }),
                        },
                        ResponseMediaType {
                            media: MediaType {
                                media_type: "application/json".into(),
                                schema: Some(reference(envelope, "/envelope")),
                                source: None,
                            },
                            projection: None,
                        },
                    ],
                    source: None,
                },
                Response {
                    status: ResponseStatus::Exact(200),
                    description: None,
                    content: vec![],
                    source: None,
                },
                Response {
                    status: ResponseStatus::Default,
                    description: None,
                    content: vec![],
                    source: None,
                },
                Response {
                    status: ResponseStatus::Exact(204),
                    description: Some("empty".into()),
                    content: vec![],
                    source: None,
                },
            ],
            servers: None,
            security: None,
            interpretation: OperationInterpretation::default(),
            source: None,
        }],
        servers: None,
        source: None,
    });
    http
}

/// Defines the composition and coordinate definitions that report errors.
fn failing_definitions(
    builder: &mut ApiBuilder,
    missing: DefinitionId,
    out_of_range: DefinitionId,
) -> (DefinitionId, DefinitionId) {
    let composition_owner = builder.reserve_definition();
    let coordinate_owner = builder.reserve_definition();

    builder
        .define(
            composition_owner,
            definition(
                "Union",
                SchemaUse::new(TypeExpr::Composition(CompositionSchema {
                    kind: CompositionKind::OneOf,
                    branches: vec![
                        reference(out_of_range, "/def/Union/branch-a"),
                        unlocated(missing),
                    ],
                    discriminator: Some(Discriminator {
                        property_name: "kind".into(),
                        mappings: vec![DiscriminatorMapping {
                            wire_value: "a".into(),
                            target: out_of_range,
                            source: Some(source("/def/Union/mapping-a")),
                        }],
                    }),
                })),
            ),
        )
        .unwrap();
    builder
        .define(
            coordinate_owner,
            definition(
                "Placement",
                coordinate(
                    out_of_range,
                    ["latitude", "longitude"],
                    " ",
                    "/def/Placement",
                ),
            ),
        )
        .unwrap();

    (composition_owner, coordinate_owner)
}
