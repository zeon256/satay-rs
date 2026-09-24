use http::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use tracing::{debug, instrument};

use crate::error::Error;

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
impl<B: AsRef<[u8]>> ResponseParts<B> {
    /// Borrows the body while retaining owned HTTP metadata in the returned parts.
    pub fn as_bytes(&self) -> ResponseParts<&[u8]> {
        ResponseParts {
            status: self.status,
            headers: self.headers.clone(),
            body: self.body.as_ref(),
        }
    }
}
/// Converts request parts into an HTTP request.
///
/// # Errors
///
/// Returns an error if the method, URI, or body cannot be converted into an HTTP request.
#[instrument(skip_all, fields(method = %method, uri = %uri))]
pub fn into_request<B>(
    RequestParts {
        method,
        uri,
        headers,
        body,
    }: RequestParts<B>,
) -> Result<http::Request<B>, Error> {
    debug!("building HTTP request");
    let mut request = http::Request::builder()
        .method(method)
        .uri(uri)
        .body(body)?;
    *request.headers_mut() = headers;
    Ok(request)
}
/// Converts request parts with an empty body into an HTTP request.
///
/// # Errors
///
/// Returns an error if the method or URI cannot be converted into an HTTP request.
#[instrument(skip_all, fields(method = %method, uri = %uri))]
pub fn into_empty_request(
    RequestParts {
        method,
        uri,
        headers,
        body: (),
    }: RequestParts<()>,
) -> Result<http::Request<Vec<u8>>, Error> {
    debug!("building empty HTTP request");
    let mut request = http::Request::builder()
        .method(method)
        .uri(uri)
        .body(vec![])?;
    *request.headers_mut() = headers;
    Ok(request)
}
/// Inserts a header into a header map.
///
/// # Errors
///
/// Returns an error if the header name or value is invalid.
pub fn insert_header(
    headers: &mut http::HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), Error> {
    headers.insert(
        HeaderName::from_bytes(name.as_bytes())?,
        HeaderValue::from_str(value)?,
    );
    Ok(())
}
#[must_use]
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
