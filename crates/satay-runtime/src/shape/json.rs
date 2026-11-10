//! JSON entry points for values described by a shape.

use std::fmt;
use std::marker::PhantomData;

use satay_storage::Storage;
use serde::de::{Deserialize, DeserializeSeed, Deserializer};

use super::{Decode, Encode, Shape, view};
use crate::error::Error;
use crate::http_parts::RequestParts;
use crate::{JsonValue, json};

/// Converts request parts whose body has shape `Sh` into a JSON HTTP request.
///
/// # Errors
/// Returns an error if JSON serialization fails or the HTTP request cannot be built.
pub fn into_json_request<'s, Sh: Encode<'s, S>, S: Storage + 's>(
    RequestParts {
        method,
        uri,
        headers,
        body,
    }: RequestParts<Sh::Value>,
) -> Result<http::Request<Vec<u8>>, Error> {
    json::into_json_request(RequestParts {
        method,
        uri,
        headers,
        body: view::<Sh, S>(&body),
    })
}

/// Converts request parts whose optional body has shape `Sh` into an HTTP request.
///
/// # Errors
/// Returns an error if JSON serialization fails or the HTTP request cannot be built.
pub fn into_optional_json_request<'s, Sh: Encode<'s, S>, S: Storage + 's>(
    RequestParts {
        method,
        uri,
        headers,
        body,
    }: RequestParts<Option<Sh::Value>>,
) -> Result<http::Request<Vec<u8>>, Error> {
    json::into_optional_json_request(RequestParts {
        method,
        uri,
        headers,
        body: body.as_ref().map(view::<Sh, S>),
    })
}

/// Decodes a JSON body of shape `Sh` into `storage`.
///
/// # Errors
/// Returns an error if the body is not valid JSON for the shape, or the storage
/// policy cannot hold a decoded value.
pub fn from_json_slice<'de, 's, Sh, S>(body: &'de [u8], storage: &'s S) -> Result<Sh::Value, Error>
where
    Sh: Decode<'de, 's, S>,
    S: Storage<Error: fmt::Display> + 's,
{
    let mut deserializer = serde_json::Deserializer::from_slice(body);
    let value = Sh::decode(&mut deserializer, storage)?;
    deserializer.end()?;
    Ok(value)
}

/// Decodes a projected JSON body of shape `Sh` into `storage`.
///
/// Projection follows [`crate::from_projected_json_slice`].
///
/// # Errors
/// Returns an error when the response does not have the configured container
/// shape, the projected JSON does not match the shape, or the storage policy
/// cannot hold a decoded value.
pub fn from_projected_json_slice<'de, 's, Sh, S>(
    body: &[u8],
    unwrap_field: &str,
    map_field: Option<&str>,
    storage: &'s S,
) -> Result<Sh::Value, Error>
where
    Sh: Decode<'de, 's, S>,
    S: Storage<Error: fmt::Display> + 's,
{
    let projected = json::project(body, unwrap_field, map_field)?;
    Ok(Sh::decode(projected, storage)?)
}

/// A [`DeserializeSeed`] that decodes shape `Sh`, treating a mismatch as `None`.
pub struct Lossy<'s, Sh, S>(&'s S, PhantomData<fn() -> Sh>);

/// Creates the seed decoding shape `Sh` into `storage`, or `None` on mismatch.
///
/// The value is read as JSON first, so only well-formed input reaches the shape.
/// A storage context need not reclaim what a rejected value already allocated.
pub fn lossy<Sh, S>(storage: &S) -> Lossy<'_, Sh, S> {
    Lossy(storage, PhantomData)
}

impl<'de, 's, Sh, S> DeserializeSeed<'de> for Lossy<'s, Sh, S>
where
    Sh: Decode<'de, 's, S>,
    S: Storage<Error: fmt::Display> + 's,
{
    type Value = Option<<Sh as Shape<'s, S>>::Value>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let value = JsonValue::deserialize(deserializer)?;
        Ok(Sh::decode(value, self.0).ok())
    }
}
