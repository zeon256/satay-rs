use crate::{DefinitionId, SourceRef};

/// One structural graph construction error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    /// A definition ID does not identify a slot in this builder.
    #[error("unknown definition {id:?}")]
    UnknownDefinition {
        /// Definition ID supplied to the builder.
        id: DefinitionId,
    },
    /// A reserved definition slot was already filled.
    #[error("definition {id:?} is already defined")]
    AlreadyDefined {
        /// Definition ID supplied to the builder.
        id: DefinitionId,
    },
    /// A reserved definition slot was not filled before finalization.
    #[error("definition {id:?} was not defined")]
    MissingDefinition {
        /// Unfilled definition ID.
        id: DefinitionId,
    },
    /// A schema use references an ID outside the builder's slot range.
    #[error(
        "definition {owner:?} references unknown definition {target:?} (location: {location:?})"
    )]
    UnresolvedReference {
        /// Definition containing the offending schema use.
        owner: DefinitionId,
        /// Out-of-range referenced definition ID.
        target: DefinitionId,
        /// Provenance attached to the offending schema use.
        location: Option<SourceRef>,
    },
}

/// All structural errors found while finalizing a semantic graph.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid semantic graph ({count} errors)", count = .0.len())]
pub struct BuildErrors(Vec<BuildError>);

impl BuildErrors {
    pub(crate) fn new(errors: Vec<BuildError>) -> Self {
        debug_assert!(!errors.is_empty());
        Self(errors)
    }

    /// Returns the construction errors in deterministic graph traversal order.
    #[must_use]
    pub fn errors(&self) -> &[BuildError] {
        &self.0
    }
}
