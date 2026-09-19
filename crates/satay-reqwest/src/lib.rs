pub use reqwest;
pub use satay_runtime;

#[cfg(feature = "blocking")]
use reqwest::blocking;
use std::{future, mem};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("satay error: {0}")]
    Satay(#[from] satay_runtime::Error),
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

pub trait ReqwestActionExt: satay_runtime::Action + Sized + Send {
    /// Sends this action and returns its owned decoded response.
    ///
    /// # Errors
    /// Returns an error if request construction, transport, body reading, or decoding fails.
    fn send_with(
        self,
        client: &reqwest::Client,
    ) -> impl future::Future<Output = Result<Self::OwnedResponse, Error>> + Send
    where
        Self: satay_runtime::OwnedAction,
        Self::RequestBody: Into<reqwest::Body> + Send,
    {
        async move {
            let response = ReqwestActionExt::send_buffered_with(self, client).await?;
            Ok(response.decode_owned()?)
        }
    }

    /// Sends this action using the supplied async `reqwest` client.
    ///
    /// # Errors
    ///
    /// The returned future resolves to an error if request construction, transport,
    /// or body reading fails. Decoding is explicit on the returned response.
    fn send_buffered_with(
        self,
        client: &reqwest::Client,
    ) -> impl future::Future<
        Output = Result<satay_runtime::BufferedResponse<Self, bytes::Bytes>, Error>,
    > + Send
    where
        Self::RequestBody: Into<reqwest::Body> + Send,
    {
        async move {
            let http_req = self.request()?.map(Into::into);
            let reqwest_req: reqwest::Request = http_req.try_into()?;

            let mut reqwest_res = client.execute(reqwest_req).await?;

            let response_parts = satay_runtime::ResponseParts {
                status: reqwest_res.status(),
                headers: mem::take(reqwest_res.headers_mut()),
                body: reqwest_res.bytes().await?,
            };

            Ok(satay_runtime::BufferedResponse::new(response_parts))
        }
    }
}

impl<T: satay_runtime::Action + Send> ReqwestActionExt for T {}

#[cfg(feature = "blocking")]
pub trait ReqwestBlockingActionExt: satay_runtime::Action + Sized {
    /// Sends this action and returns its owned decoded response.
    ///
    /// # Errors
    /// Returns an error if request construction, transport, body reading, or decoding fails.
    fn send_with(self, client: &blocking::Client) -> Result<Self::OwnedResponse, Error>
    where
        Self: satay_runtime::OwnedAction,
        Self::RequestBody: Into<blocking::Body>,
    {
        let response = ReqwestBlockingActionExt::send_buffered_with(self, client)?;
        Ok(response.decode_owned()?)
    }

    /// Sends this action using the supplied blocking `reqwest` client.
    ///
    /// # Errors
    ///
    /// Returns an error if request construction, transport, or body reading fails.
    /// Decoding is explicit on the returned response.
    fn send_buffered_with(
        self,
        client: &blocking::Client,
    ) -> Result<satay_runtime::BufferedResponse<Self, bytes::Bytes>, Error>
    where
        Self::RequestBody: Into<blocking::Body>,
    {
        let http_req = self.request()?.map(Into::into);
        let reqwest_req: blocking::Request = http_req.try_into()?;

        let mut reqwest_res = client.execute(reqwest_req)?;

        let response_parts = satay_runtime::ResponseParts {
            status: reqwest_res.status(),
            headers: mem::take(reqwest_res.headers_mut()),
            body: reqwest_res.bytes()?,
        };

        Ok(satay_runtime::BufferedResponse::new(response_parts))
    }
}

#[cfg(feature = "blocking")]
impl<T: satay_runtime::Action> ReqwestBlockingActionExt for T {}

#[cfg(test)]
mod tests;
