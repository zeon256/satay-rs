use std::io;

use http::{header, status};
use tokio_tungstenite::tungstenite;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("satay error: {0}")]
    Satay(#[from] satay_runtime::Error),

    #[error("websocket error: {0}")]
    WebSocket(Box<tungstenite::Error>),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("invalid HTTP status code: {0}")]
    InvalidStatus(#[from] status::InvalidStatusCode),

    #[error("invalid HTTP header name: {0}")]
    InvalidHeaderName(#[from] header::InvalidHeaderName),

    #[error("invalid HTTP header value: {0}")]
    InvalidHeaderValue(#[from] header::InvalidHeaderValue),

    #[error("websocket closed before a response arrived")]
    Closed,
}

impl From<tungstenite::Error> for Error {
    fn from(error: tungstenite::Error) -> Self {
        Self::WebSocket(Box::new(error))
    }
}
