use http::header;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to build HTTP message: {0}")]
    Http(#[from] http::Error),

    #[error("invalid HTTP header value: {0}")]
    InvalidHeaderValue(#[from] header::InvalidHeaderValue),

    #[error("invalid HTTP header name: {0}")]
    InvalidHeaderName(#[from] header::InvalidHeaderName),

    #[error("missing required field `{0}`")]
    MissingRequired(&'static str),

    #[error("{0}")]
    InvalidResponse(&'static str),

    #[cfg(feature = "json")]
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
