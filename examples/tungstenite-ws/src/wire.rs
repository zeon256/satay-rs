use http::header::{HeaderName, HeaderValue};
use http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WireRequest {
    pub(crate) id: u64,
    pub(crate) method: String,
    pub(crate) uri: String,
    pub(crate) headers: Vec<WireHeader>,
    pub(crate) body: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WireResponse {
    pub(crate) id: u64,
    pub(crate) status: u16,
    pub(crate) headers: Vec<WireHeader>,
    pub(crate) body: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WireHeader {
    pub(crate) name: String,
    pub(crate) value: Vec<u8>,
}

impl WireRequest {
    pub(crate) fn from_http(id: u64, request: http::Request<Vec<u8>>) -> Self {
        let (parts, body) = request.into_parts();
        Self {
            id,
            method: parts.method.to_string(),
            uri: parts.uri.to_string(),
            headers: headers_to_wire(&parts.headers),
            body,
        }
    }
}

impl WireResponse {
    pub(crate) fn into_response_parts(
        self,
    ) -> Result<satay_runtime::ResponseParts<Vec<u8>>, Error> {
        Ok(satay_runtime::ResponseParts {
            status: StatusCode::from_u16(self.status)?,
            headers: wire_to_headers(self.headers)?,
            body: self.body,
        })
    }
}

fn headers_to_wire(headers: &HeaderMap) -> Vec<WireHeader> {
    headers
        .iter()
        .map(|(name, value)| WireHeader {
            name: name.as_str().to_owned(),
            value: value.as_bytes().to_vec(),
        })
        .collect()
}

fn wire_to_headers(headers: Vec<WireHeader>) -> Result<HeaderMap, Error> {
    let mut out = HeaderMap::new();
    for header in headers {
        out.append(
            HeaderName::from_bytes(header.name.as_bytes())?,
            HeaderValue::from_bytes(&header.value)?,
        );
    }
    Ok(out)
}
