#![forbid(unsafe_code)]

use http::header::{self, CONTENT_TYPE};
#[cfg(feature = "json")]
use serde::de;

use tracing::instrument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestParts<B> {
    pub method: http::Method,
    pub uri: String,
    pub headers: http::HeaderMap,
    pub body: B,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseParts<B> {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: B,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to build HTTP message: {0}")]
    Http(#[from] http::Error),

    #[error("invalid HTTP header value: {0}")]
    InvalidHeaderValue(#[from] header::InvalidHeaderValue),

    #[error("missing required field `{0}`")]
    MissingRequired(&'static str),

    #[error("{0}")]
    InvalidResponse(&'static str),

    #[cfg(feature = "json")]
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[instrument(skip_all, fields(method = %method, uri = %uri))]
pub fn into_request<B>(
    RequestParts {
        method,
        uri,
        headers,
        body,
    }: RequestParts<B>,
) -> Result<http::Request<B>, Error> {
    tracing::debug!("building HTTP request");
    let mut request = http::Request::builder()
        .method(method)
        .uri(uri)
        .body(body)?;
    *request.headers_mut() = headers;
    Ok(request)
}

#[instrument(skip_all, fields(method = %method, uri = %uri))]
pub fn into_empty_request(
    RequestParts {
        method,
        uri,
        headers,
        body: _,
    }: RequestParts<()>,
) -> Result<http::Request<Vec<u8>>, Error> {
    tracing::debug!("building empty HTTP request");
    let mut request = http::Request::builder()
        .method(method)
        .uri(uri)
        .body(vec![])?;
    *request.headers_mut() = headers;
    Ok(request)
}

#[cfg(feature = "json")]
#[instrument(skip_all, fields(method = %method, uri = %uri))]
pub fn into_json_request<T>(
    RequestParts {
        method,
        uri,
        headers,
        body,
    }: RequestParts<T>,
) -> Result<http::Request<Vec<u8>>, Error>
where
    T: serde::Serialize,
{
    tracing::debug!("building JSON HTTP request");
    let body = serde_json::to_vec(&body)?;
    let mut request = http::Request::builder()
        .method(method)
        .uri(uri)
        .body(body)?;
    *request.headers_mut() = headers;
    if !request.headers().contains_key(CONTENT_TYPE) {
        request.headers_mut().insert(
            CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
    }
    Ok(request)
}

#[cfg(feature = "json")]
#[instrument(skip_all, fields(method = %method, uri = %uri))]
pub fn into_optional_json_request<T>(
    RequestParts {
        method,
        uri,
        headers,
        body,
    }: RequestParts<Option<T>>,
) -> Result<http::Request<Vec<u8>>, Error>
where
    T: serde::Serialize,
{
    match body {
        Some(body) => into_json_request(RequestParts {
            method,
            uri,
            headers,
            body,
        }),
        None => into_empty_request(RequestParts {
            method,
            uri,
            headers,
            body: (),
        }),
    }
}

#[cfg(feature = "json")]
#[instrument(skip_all)]
pub fn from_json_slice<T>(body: &[u8]) -> Result<T, Error>
where
    T: de::DeserializeOwned,
{
    tracing::debug!("deserializing JSON response");
    Ok(serde_json::from_slice(body)?)
}

pub fn append_path_segment(out: &mut String, value: &str) {
    append_percent_encoded(out, value.as_bytes());
}

pub fn append_query_pair(out: &mut String, first: &mut bool, key: &str, value: &str) {
    if *first {
        out.push('?');
        *first = false;
    } else {
        out.push('&');
    }
    append_percent_encoded(out, key.as_bytes());
    out.push('=');
    append_percent_encoded(out, value.as_bytes());
}

pub fn has_json_content_type(headers: &http::HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_json_media_type)
}

fn is_json_media_type(value: &str) -> bool {
    let media_type = value.split(';').next().unwrap_or(value).trim();
    if media_type.eq_ignore_ascii_case("application/json") {
        return true;
    }

    let Some((_, subtype)) = media_type.rsplit_once('/') else {
        return false;
    };
    ends_with_ignore_ascii_case(subtype, "+json")
}

fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    let value = value.as_bytes();
    let suffix = suffix.as_bytes();
    value.len() >= suffix.len() && value[value.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

fn append_percent_encoded(out: &mut String, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for &byte in bytes {
        if is_unreserved(byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
}

const fn is_unreserved(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_path_segments() {
        let mut out = String::new();
        append_path_segment(&mut out, "a/b c");
        assert_eq!(out, "a%2Fb%20c");
    }

    #[test]
    fn appends_query_pairs() {
        let mut out = String::from("/pets");
        let mut first = true;
        append_query_pair(&mut out, &mut first, "tag name", "small/dog");
        append_query_pair(&mut out, &mut first, "limit", "10");
        assert_eq!(out, "/pets?tag%20name=small%2Fdog&limit=10");
    }

    #[test]
    fn recognizes_json_content_types() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            http::HeaderValue::from_static("application/problem+json; charset=utf-8"),
        );
        assert!(has_json_content_type(&headers));
    }
}
