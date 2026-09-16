use la_arena::Arena;

use crate::{
    AdditionalProperties, Api, BuildError, BuildErrors, Definition, DefinitionId, GraphOwner,
    HttpApi, SchemaUse, StringInterpretation, TypeExpr,
};

/// Allocates definitions and finalizes a semantic schema graph.
///
/// Reservation supports forward and cyclic references. IDs belong only to the
/// builder/API mapping that issued them.
#[derive(Debug, Default)]
pub struct ApiBuilder {
    definitions: Arena<Option<Definition>>,
    http: HttpApi,
}

impl ApiBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserves a definition slot and returns its future definition ID.
    pub fn reserve_definition(&mut self) -> DefinitionId {
        DefinitionId::from_slot_index(self.definitions.alloc(None))
    }

    /// Allocates an already complete definition and returns its ID.
    pub fn add_definition(&mut self, definition: Definition) -> DefinitionId {
        DefinitionId::from_slot_index(self.definitions.alloc(Some(definition)))
    }

    /// Fills one reserved definition slot.
    ///
    /// Unknown and already-filled IDs leave the builder unchanged. The supplied
    /// definition is consumed in either error case.
    ///
    /// # Errors
    ///
    /// Returns `UnknownDefinition` when `id` does not identify a slot in this
    /// builder, or `AlreadyDefined` when the reserved slot is already filled.
    pub fn define(&mut self, id: DefinitionId, definition: Definition) -> Result<(), BuildError> {
        if id.raw_index() >= self.definitions.len() {
            return Err(BuildError::UnknownDefinition { id });
        }

        let slot = &mut self.definitions[id.slot_index()];
        if slot.is_some() {
            return Err(BuildError::AlreadyDefined { id });
        }

        *slot = Some(definition);
        Ok(())
    }

    /// Replaces the complete HTTP record with `http`.
    ///
    /// The previous record is dropped normally; no merge or validation occurs
    /// here. HTTP roots carry no IDs.
    pub fn set_http(&mut self, http: HttpApi) {
        self.http = http;
    }

    /// Validates graph integrity and returns an immutable graph.
    ///
    /// This checks only slot completeness and schema-edge ranges. It does not
    /// evaluate schema satisfiability, compile patterns, compare bounds, reject
    /// duplicate wire properties, validate response ranges or media syntax, or
    /// select backend representations.
    ///
    /// # Errors
    ///
    /// Returns every structural error found: unfilled definition slots and
    /// schema uses referencing IDs outside this builder's slot range.
    ///
    /// # Panics
    ///
    /// Never panics in valid use. The single internal `expect` is guarded by
    /// the slot-completeness check earlier in this function; it can only fire
    /// if that check was bypassed.
    pub fn finish(self) -> Result<Api, BuildErrors> {
        let slot_count = self.definitions.len();
        let mut errors = vec![];

        for (slot_id, definition) in self.definitions.iter() {
            let id = DefinitionId::from_slot_index(slot_id);
            match definition {
                Some(definition) => {
                    inspect_use(
                        GraphOwner::Definition(id),
                        &definition.schema,
                        slot_count,
                        &mut errors,
                    );
                }
                None => errors.push(BuildError::MissingDefinition { id }),
            }
        }

        for (path_index, path) in self.http.paths.iter().enumerate() {
            for parameter in &path.parameters {
                inspect_use(
                    GraphOwner::Path { index: path_index },
                    &parameter.schema,
                    slot_count,
                    &mut errors,
                );
            }
            for (operation_index, operation) in path.operations.iter().enumerate() {
                let owner = GraphOwner::Operation {
                    path_index,
                    operation_index,
                };
                for parameter in &operation.parameters {
                    inspect_use(owner, &parameter.schema, slot_count, &mut errors);
                }
                if let Some(request_body) = &operation.request_body {
                    for media in &request_body.content {
                        if let Some(schema) = &media.schema {
                            inspect_use(owner, schema, slot_count, &mut errors);
                        }
                    }
                }
                for response in &operation.responses {
                    for media in &response.content {
                        if let Some(schema) = &media.media.schema {
                            inspect_use(owner, schema, slot_count, &mut errors);
                        }
                        if let Some(projection) = &media.projection {
                            inspect_use(owner, &projection.output, slot_count, &mut errors);
                        }
                    }
                }
            }
        }

        if !errors.is_empty() {
            return Err(BuildErrors::new(errors));
        }

        let mut definitions = Arena::with_capacity(slot_count);
        for (slot_id, definition) in self.definitions {
            let expected_id = DefinitionId::from_slot_index(slot_id);
            let definition = definition.expect("all definition slots were checked as complete");
            let actual_id = DefinitionId::from_definition_index(definitions.alloc(definition));
            debug_assert_eq!(actual_id, expected_id);
        }

        Ok(Api {
            definitions,
            http: self.http,
        })
    }
}

fn inspect_use(
    owner: GraphOwner,
    schema_use: &SchemaUse,
    slot_count: usize,
    errors: &mut Vec<BuildError>,
) {
    match &schema_use.ty {
        TypeExpr::Ref(target) if target.raw_index() >= slot_count => {
            errors.push(BuildError::UnresolvedReference {
                owner,
                target: *target,
                location: schema_use.annotations.source.clone(),
            });
        }
        TypeExpr::Array(array) => {
            inspect_use(owner, &array.items, slot_count, errors);
        }
        TypeExpr::Object(object) => {
            for property in &object.properties {
                inspect_use(owner, &property.value, slot_count, errors);
            }
            if let AdditionalProperties::Schema(value) = &object.additional_properties {
                inspect_use(owner, value, slot_count, errors);
            }
        }
        TypeExpr::Composition(composition) => {
            for branch in &composition.branches {
                inspect_use(owner, branch, slot_count, errors);
            }
            if let Some(discriminator) = &composition.discriminator {
                for mapping in &discriminator.mappings {
                    if mapping.target.raw_index() >= slot_count {
                        errors.push(BuildError::UnresolvedReference {
                            owner,
                            target: mapping.target,
                            location: mapping.source.clone(),
                        });
                    }
                }
            }
        }
        TypeExpr::String(schema) => {
            if let StringInterpretation::Coordinates(coordinates) = &schema.interpretation {
                let target = coordinates.target();
                if target.raw_index() >= slot_count {
                    errors.push(BuildError::UnresolvedReference {
                        owner,
                        target,
                        location: schema_use.annotations.source.clone(),
                    });
                }
            }
        }
        TypeExpr::Invalid(_)
        | TypeExpr::Integer(_)
        | TypeExpr::Number(_)
        | TypeExpr::Boolean
        | TypeExpr::Null
        | TypeExpr::AnyJson
        | TypeExpr::Ref(_) => {}
    }
}
