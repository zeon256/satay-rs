mod parse;
mod validation;

pub use parse::ParseError;
pub use validation::ValidationError;

/// All errors that can occur during code generation.
///
/// This enum is [`non_exhaustive`](https://doc.rust-lang.org/reference/attributes/type_system.html)
/// so new variants may be added in future releases without a semver break.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// An error that occurred while parsing an OpenAPI document.
    ///
    /// See [`ParseError`] for the full list of parse-related errors.
    #[error(transparent)]
    Parse(#[from] ParseError),

    /// An error that occurred while validating an OpenAPI document.
    ///
    /// See [`ValidationError`] for the full list of validation-related errors.
    #[error(transparent)]
    Validation(#[from] ValidationError),

    /// An internal compiler-stage failure that could not be represented by an
    /// existing parse or validation diagnostic.
    #[error("internal code generation error: {message}")]
    Internal {
        /// Description of the failed compiler invariant.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn internal_errors_have_stable_context() {
        let error = Error::Internal {
            message: "semantic graph invariant failed".to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "internal code generation error: semantic graph invariant failed"
        );
    }
}
