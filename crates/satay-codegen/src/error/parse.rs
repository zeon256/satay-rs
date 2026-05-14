/// Errors that can occur while parsing an OpenAPI document.
///
/// This enum is [`non_exhaustive`](https://doc.rust-lang.org/reference/attributes/type_system.html)
/// so new variants may be added in future releases without a semver break.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    /// Failed to parse the raw OpenAPI YAML or JSON input.
    ///
    /// Error message: `failed to parse OpenAPI YAML/JSON: {0}`
    #[error("failed to parse OpenAPI YAML/JSON: {0}")]
    Document(#[source] serde_yaml::Error),

    /// Failed to normalize the parsed OpenAPI document (e.g. during internal JSON conversion).
    ///
    /// Error message: `failed to normalize OpenAPI document: {0}`
    #[error("failed to normalize OpenAPI document: {0}")]
    NormalizeDocument(#[source] serde_json::Error),
}
