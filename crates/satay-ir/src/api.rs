use la_arena::Arena;

use crate::{Definition, DefinitionId, HttpApi};

/// A structurally finalized semantic schema graph.
///
/// Definition IDs are local to the builder/API mapping that allocated them.
/// Out-of-range lookup is checked defensively, but same-index IDs from another
/// graph cannot be detected as foreign.
#[derive(Debug, Clone)]
pub struct Api {
    pub(crate) definitions: Arena<Definition>,
    pub(crate) http: HttpApi,
}

impl Api {
    /// Returns the definition selected by `id`, or `None` when its raw index is
    /// outside this graph.
    #[must_use]
    pub fn definition(&self, id: DefinitionId) -> Option<&Definition> {
        if id.raw_index() >= self.definitions.len() {
            return None;
        }

        Some(&self.definitions[id.definition_index()])
    }

    /// Returns the immutable HTTP record.
    ///
    /// Paths, operations, and metadata appear in caller order; no effective
    /// merging or filtering has been applied.
    #[must_use]
    pub fn http(&self) -> &HttpApi {
        &self.http
    }

    /// Iterates over definitions in allocation order.
    pub fn definitions(&self) -> impl Iterator<Item = (DefinitionId, &Definition)> + '_ {
        self.definitions
            .iter()
            .map(|(id, definition)| (DefinitionId::from_definition_index(id), definition))
    }
}
