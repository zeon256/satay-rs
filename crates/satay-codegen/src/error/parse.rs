#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    #[error("failed to parse OpenAPI YAML/JSON: {0}")]
    Document(#[source] serde_yaml::Error),

    #[error("failed to normalize OpenAPI document: {0}")]
    NormalizeDocument(#[source] serde_json::Error),
}
