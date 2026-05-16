#![forbid(unsafe_code)]

use http::header::{self, CONTENT_TYPE, HeaderName, HeaderValue};
#[cfg(feature = "json")]
use serde::de;
pub use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use tracing::{debug, instrument};

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

pub trait Action {
    type Response;

    fn request(self) -> Result<http::Request<Vec<u8>>, Error>;
    fn decode<B: AsRef<[u8]>>(response: ResponseParts<B>) -> Result<Self::Response, Error>;
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
    debug!("building HTTP request");
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
    debug!("building empty HTTP request");
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
    debug!("deserializing JSON response");
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

pub fn format_offset_datetime(value: &OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| value.to_string())
}

#[cfg(feature = "serde")]
pub mod serde_string {
    use std::fmt;
    use std::str::FromStr;

    use serde::Deserialize;
    use serde::de::Error as DeError;
    use time::format_description::well_known::Rfc3339;

    use crate::OffsetDateTime;

    macro_rules! string_from_str_module {
        ($module:ident, $ty:ty) => {
            pub mod $module {
                pub fn serialize<S>(value: &$ty, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    super::serialize_display(value, serializer)
                }

                pub fn deserialize<'de, D>(deserializer: D) -> Result<$ty, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    super::deserialize_from_str(deserializer)
                }

                pub mod option {
                    pub fn serialize<S>(
                        value: &Option<$ty>,
                        serializer: S,
                    ) -> Result<S::Ok, S::Error>
                    where
                        S: serde::Serializer,
                    {
                        super::super::serialize_option_display(value, serializer)
                    }

                    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<$ty>, D::Error>
                    where
                        D: serde::Deserializer<'de>,
                    {
                        super::super::deserialize_option_from_str(deserializer)
                    }
                }
            }
        };
    }

    macro_rules! string_float_module {
        ($module:ident, $ty:ty) => {
            pub mod $module {
                use serde::Deserialize;
                use serde::de::Error as DeError;

                pub fn serialize<S>(value: &$ty, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    super::serialize_display(value, serializer)
                }

                pub fn deserialize<'de, D>(deserializer: D) -> Result<$ty, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    let value = <String as Deserialize>::deserialize(deserializer)?;
                    fast_float::parse::<$ty, _>(&value).map_err(DeError::custom)
                }

                pub mod option {
                    use serde::Deserialize;
                    use serde::de::Error as DeError;

                    pub fn serialize<S>(
                        value: &Option<$ty>,
                        serializer: S,
                    ) -> Result<S::Ok, S::Error>
                    where
                        S: serde::Serializer,
                    {
                        super::super::serialize_option_display(value, serializer)
                    }

                    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<$ty>, D::Error>
                    where
                        D: serde::Deserializer<'de>,
                    {
                        let value = <Option<String> as Deserialize>::deserialize(deserializer)?;
                        value
                            .map(|value| {
                                fast_float::parse::<$ty, _>(&value).map_err(DeError::custom)
                            })
                            .transpose()
                    }
                }
            }
        };
    }

    string_from_str_module!(as_u8, u8);
    string_from_str_module!(as_u16, u16);
    string_from_str_module!(as_u32, u32);
    string_from_str_module!(as_u64, u64);
    string_from_str_module!(as_i8, i8);
    string_from_str_module!(as_i16, i16);
    string_from_str_module!(as_i32, i32);
    string_from_str_module!(as_i64, i64);
    string_float_module!(as_f32, f32);
    string_float_module!(as_f64, f64);

    pub mod as_offset_datetime {
        use serde::Deserialize;
        use serde::de::Error as DeError;
        use serde::ser::Error as SerError;

        use super::*;

        pub fn serialize<S>(value: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let value = value.format(&Rfc3339).map_err(SerError::custom)?;
            serializer.serialize_str(&value)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let value = <String as Deserialize>::deserialize(deserializer)?;
            OffsetDateTime::parse(&value, &Rfc3339).map_err(DeError::custom)
        }

        pub mod option {
            use serde::Deserialize;
            use serde::de::Error as DeError;

            use super::*;

            pub fn serialize<S>(
                value: &Option<OffsetDateTime>,
                serializer: S,
            ) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                match value {
                    Some(value) => super::serialize(value, serializer),
                    None => serializer.serialize_none(),
                }
            }

            pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<OffsetDateTime>, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = <Option<String> as Deserialize>::deserialize(deserializer)?;
                value
                    .map(|value| OffsetDateTime::parse(&value, &Rfc3339).map_err(DeError::custom))
                    .transpose()
            }
        }
    }

    fn serialize_display<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: fmt::Display,
        S: serde::Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    fn serialize_option_display<T, S>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: fmt::Display,
        S: serde::Serializer,
    {
        match value {
            Some(value) => serialize_display(value, serializer),
            None => serializer.serialize_none(),
        }
    }

    fn deserialize_from_str<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: fmt::Display,
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse::<T>().map_err(DeError::custom)
    }

    fn deserialize_option_from_str<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        T: FromStr,
        T::Err: fmt::Display,
        D: serde::Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        value
            .map(|value| value.parse::<T>().map_err(DeError::custom))
            .transpose()
    }
}

#[cfg(feature = "json")]
pub mod treat_error_as_none {
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S, T>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        match value {
            Some(inner) => inner.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match T::deserialize(value) {
            Ok(parsed) => Ok(Some(parsed)),
            Err(_) => Ok(None),
        }
    }
}

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

    #[test]
    fn response_parts_holds_status_headers_body() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        let body = br#"{"ok":true}"#.to_vec();
        let parts = ResponseParts {
            status: http::StatusCode::OK,
            headers,
            body,
        };
        assert_eq!(parts.status, http::StatusCode::OK);
        assert_eq!(parts.headers.get(CONTENT_TYPE).unwrap(), "application/json");
        assert_eq!(parts.body, br#"{"ok":true}"#);
    }
}
