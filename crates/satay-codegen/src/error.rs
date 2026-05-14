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
}
