use std::fmt::{self, Debug, Formatter};
use std::marker;

use crate::error::Error;
use crate::http_parts::ResponseParts;

pub trait Action {
    /// Container produced when encoding the request.
    type RequestBody;
    /// Decoded response, potentially borrowing the supplied body.
    type Response<'de>;

    /// Builds the HTTP request for this action.
    ///
    /// # Errors
    ///
    /// Returns an error if request construction or required input validation fails.
    fn request(self) -> Result<http::Request<Self::RequestBody>, Error>;

    /// Decodes the HTTP response body into this action's response type.
    ///
    /// # Errors
    ///
    /// Returns an error if the response is invalid or cannot be decoded.
    fn decode(response: ResponseParts<&[u8]>) -> Result<Self::Response<'_>, Error>;
}
/// An action that can decode a response independently of the input buffer's lifetime.
///
/// Transports use this contract for their one-step `send_with` methods. Actions
/// that only support borrowing can implement [`Action`] alone and use buffering.
pub trait OwnedAction: Action {
    /// Decoded value that can outlive the HTTP response buffer.
    type OwnedResponse;

    /// Decodes without retaining references to the supplied response body.
    ///
    /// # Errors
    /// Returns an error if the response is invalid or cannot be decoded.
    fn decode_owned(response: ResponseParts<&[u8]>) -> Result<Self::OwnedResponse, Error>;
}
/// A transport-owned response whose decoded model may borrow its body.
pub struct BufferedResponse<A, B> {
    parts: ResponseParts<B>,
    action: marker::PhantomData<fn() -> A>,
}

impl<A, B: Debug> Debug for BufferedResponse<A, B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufferedResponse")
            .field("parts", &self.parts)
            .finish()
    }
}

impl<A, B> BufferedResponse<A, B> {
    /// Associates transport response parts with their action's decoder.
    pub fn new(parts: ResponseParts<B>) -> Self {
        Self {
            parts,
            action: marker::PhantomData,
        }
    }

    /// Inspects the status, headers, and original body container.
    pub fn parts(&self) -> &ResponseParts<B> {
        &self.parts
    }

    /// Recovers the original transport response without decoding.
    pub fn into_parts(self) -> ResponseParts<B> {
        self.parts
    }
}

impl<A: Action, B: AsRef<[u8]>> BufferedResponse<A, B> {
    /// Decodes a model borrowing this response, if supported by the action.
    ///
    /// # Errors
    /// Returns the action's decoding error for invalid response data.
    pub fn decode(&self) -> Result<A::Response<'_>, Error> {
        A::decode(self.parts.as_bytes())
    }
}

impl<A: OwnedAction, B: AsRef<[u8]>> BufferedResponse<A, B> {
    /// Decodes an owned response and releases the transport buffer.
    ///
    /// # Errors
    /// Returns the action's decoding error for invalid response data.
    pub fn decode_owned(self) -> Result<A::OwnedResponse, Error> {
        let ResponseParts {
            status,
            headers,
            body,
        } = self.parts;
        A::decode_owned(ResponseParts {
            status,
            headers,
            body: body.as_ref(),
        })
    }
}
