use http::header::CONTENT_TYPE;

use serde::de;
use tracing::{debug, instrument};

use crate::Error;
use crate::JsonValue;
use crate::http_parts::RequestParts;
use crate::http_parts::into_empty_request;

/// Converts serializable request parts into a JSON HTTP request.
///
/// # Errors
///
/// Returns an error if JSON serialization fails or the HTTP request cannot be built.
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
    debug!("building JSON HTTP request");

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
/// Converts optional serializable request parts into a JSON HTTP request.
///
/// # Errors
///
/// Returns an error if JSON serialization fails or the HTTP request cannot be built.
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
/// Deserializes a JSON response body from bytes.
///
/// # Errors
///
/// Returns an error if the body is not valid JSON for `T`.
#[cfg(feature = "json")]
#[instrument(skip_all)]
pub fn from_json_slice<'de, T>(body: &'de [u8]) -> Result<T, Error>
where
    T: serde::Deserialize<'de>,
{
    debug!("deserializing JSON response");
    Ok(serde_json::from_slice(body)?)
}
/// Deserializes a projected JSON response body from bytes.
///
/// The top-level `unwrap_field` is selected first. When `map_field` is set, the
/// unwrapped value must be an array of objects and that field is selected from
/// every item. Missing fields become JSON `null`, allowing the projected Rust
/// type's normal serde rules to distinguish optional and required values.
///
/// # Errors
///
/// Returns an error when the response does not have the configured container
/// shape or when the projected JSON cannot be deserialized as `T`.
#[cfg(feature = "json")]
#[instrument(skip_all)]
pub fn from_projected_json_slice<T>(
    body: &[u8],
    unwrap_field: &str,
    map_field: Option<&str>,
) -> Result<T, Error>
where
    T: de::DeserializeOwned,
{
    debug!(
        unwrap_field,
        map_field, "deserializing projected JSON response"
    );
    let mut value = serde_json::from_slice::<JsonValue>(body)?;
    let object = value.as_object_mut().ok_or(Error::InvalidResponse(
        "response projection expected a top-level JSON object",
    ))?;
    let mut projected = object.remove(unwrap_field).unwrap_or(JsonValue::Null);

    if let Some(map_field) = map_field {
        projected = match projected {
            JsonValue::Null => JsonValue::Null,
            JsonValue::Array(items) => JsonValue::Array(
                items
                    .into_iter()
                    .map(|mut item| {
                        let object = item.as_object_mut().ok_or(Error::InvalidResponse(
                            "response projection expected array items to be JSON objects",
                        ))?;
                        Ok(object.remove(map_field).unwrap_or(JsonValue::Null))
                    })
                    .collect::<Result<Vec<_>, Error>>()?,
            ),
            _ => {
                return Err(Error::InvalidResponse(
                    "response projection expected the unwrapped field to be a JSON array",
                ));
            }
        };
    }

    Ok(serde_json::from_value(projected)?)
}
