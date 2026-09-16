//! Structured, source-aware errors for the OpenAPI-to-IR frontend.
//!
//! Visibility is limited to `crate::parse`: this frontend is private and
//! test-gated, so no public error export exists yet.

use crate::error::{ParseError, ValidationError};
use crate::parse::diagnostic;

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
        use satay_ir::DiagnosticKind;
        let kind = match self {
            Self::Validation { source, .. } => return diagnostic::retain(source),
            Self::Interpretation { location, source } => DiagnosticKind::Interpretation {
                location: location.clone(),
                source: source.clone(),
            },
            Self::ExcludedDefinition { name, location } => DiagnosticKind::ExcludedDefinition {
                name: name.clone(),
                location: location.clone(),
            },
            Self::ApiKeyLocation { value, location } => DiagnosticKind::ApiKeyLocation {
                value: value.clone(),
                location: location.clone(),
            },
            error => panic!("non-semantic failure cannot be deferred: {error:?}"),
        };
        satay_ir::Diagnostic {
            kind,
            message: self.to_string(),
        }
    }
}
