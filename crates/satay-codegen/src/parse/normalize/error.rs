//! Structured, source-aware errors for the OpenAPI-to-IR frontend.
//!
//! Visibility is limited to `crate::parse`: this frontend is private and
//! test-gated, so no public error export exists yet.

use crate::error::{ParseError, ValidationError};

/// Errors raised while normalizing a resolved OpenAPI document into
/// [`satay_ir::Api`].
#[derive(Debug, thiserror::Error)]
pub(in crate::parse) enum NormalizeError {
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("{source} (location: {location:?})")]
    Validation {
        location: satay_ir::SourceRef,
        #[source]
        source: Box<ValidationError>,
    },
    #[error("{source} (location: {location:?})")]
    Interpretation {
        location: satay_ir::SourceRef,
        #[source]
        source: satay_ir::InterpretationError,
    },
    #[error(transparent)]
    Definition(#[from] satay_ir::BuildError),
    #[error(transparent)]
    Graph(#[from] satay_ir::BuildErrors),
    #[error("definition `{name}` was excluded by operation selection (location: {location:?})")]
    ExcludedDefinition {
        name: String,
        location: satay_ir::SourceRef,
    },
    #[error("unsupported API key location `{value}` (location: {location:?})")]
    ApiKeyLocation {
        value: String,
        location: satay_ir::SourceRef,
    },
}

impl NormalizeError {
    pub(super) fn diagnostic(&self) -> satay_ir::Diagnostic {
        let (code, message) = match self {
            Self::Validation { source, .. } => (format!("{source:?}"), source.to_string()),
            Self::Interpretation { source, .. } => (format!("{source:?}"), source.to_string()),
            _ => (format!("{self:?}"), self.to_string()),
        };
        satay_ir::Diagnostic {
            code: code
                .split([' ', '{', '('])
                .next()
                .unwrap_or("Frontend")
                .to_owned(),
            message,
        }
    }
}
