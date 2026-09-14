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
    #[error("{owner:?} references unknown definition {target:?} (location: {location:?})")]
    UnresolvedReference {
        /// Graph node containing the offending schema use.
        owner: GraphOwner,
        /// Out-of-range referenced definition ID.
        target: DefinitionId,
        /// Provenance attached to the offending schema use.
        location: Option<SourceRef>,
    },
}

/// The graph node owning one checked schema use.
///
/// Indices identify vector positions in the submitted HTTP tree, not portable
/// IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphOwner {
    /// A definition slot owner.
    Definition(DefinitionId),
    /// A path-level owner.
    Path {
        /// Position of the path in the submitted `HttpApi.paths`.
        index: usize,
    },
    /// An operation-level owner.
    Operation {
        /// Position of the path in the submitted `HttpApi.paths`.
        path_index: usize,
        /// Position of the operation in the path's `operations`.
        operation_index: usize,
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
