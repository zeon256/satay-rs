/// Errors that can occur while parsing an OpenAPI document.
///
/// This enum is [`non_exhaustive`](https://doc.rust-lang.org/reference/attributes/type_system.html)
/// so new variants may be added in future releases without a semver break.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    /// Failed to parse the OpenAPI YAML document.
    ///
    /// Error message: `failed to parse OpenAPI YAML document: {0}`
    #[error("failed to parse OpenAPI YAML document: {0}")]
    OpenApiDocument(#[from] yaml_serde::Error),
}
