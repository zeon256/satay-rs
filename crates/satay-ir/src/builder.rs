use la_arena::Arena;

use crate::{
    AdditionalProperties, Api, BuildError, BuildErrors, Definition, DefinitionId, SchemaUse,
    TypeExpr,
};

/// Allocates definitions and finalizes a semantic schema graph.
///
/// Reservation supports forward and cyclic references. IDs belong only to the
/// builder/API mapping that issued them.
#[derive(Debug, Default)]
pub struct ApiBuilder {
    definitions: Arena<Option<Definition>>,
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

    /// Validates graph integrity and returns an immutable graph.
    ///
    /// This checks only slot completeness and reference ranges. It does not
    /// evaluate schema satisfiability, compile patterns, compare bounds, reject
    /// duplicate wire properties, or select backend representations.
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
            let owner = DefinitionId::from_slot_index(slot_id);
            match definition {
                Some(definition) => {
                    inspect_use(owner, &definition.schema, slot_count, &mut errors);
                }
                None => errors.push(BuildError::MissingDefinition { id: owner }),
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

        Ok(Api { definitions })
    }
}

fn inspect_use(
    owner: DefinitionId,
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
        TypeExpr::String(_)
        | TypeExpr::Integer(_)
        | TypeExpr::Number(_)
        | TypeExpr::Boolean
        | TypeExpr::AnyJson
        | TypeExpr::Ref(_) => {}
    }
}
