pub use satay_runtime;
pub use ureq;

use std::io::{self, Read};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("satay error: {0}")]
    Satay(#[from] satay_runtime::Error),
    #[error("ureq error: {0}")]
    Ureq(#[from] Box<ureq::Error>),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub trait UreqActionExt: satay_runtime::Action + Sized {
    /// Sends this action and returns its owned decoded response.
    ///
    /// # Errors
    /// Returns an error if request construction, transport, body reading, or decoding fails.
    fn send_with(self, agent: &ureq::Agent) -> Result<Self::OwnedResponse, Error>
    where
        Self: satay_runtime::OwnedAction,
        Self::RequestBody: AsRef<[u8]>,
    {
        let response = UreqActionExt::send_buffered_with(self, agent)?;
        Ok(response.decode_owned()?)
    }

    /// Sends this action using the supplied `ureq` agent.
    ///
    /// # Errors
    ///
    /// Returns an error if request construction, transport, or body reading fails.
    fn send_buffered_with(
        self,
        agent: &ureq::Agent,
    ) -> Result<satay_runtime::BufferedResponse<Self, Vec<u8>>, Error>
    where
        Self::RequestBody: AsRef<[u8]>,
    {
        let http_req = self.request()?;
        let (parts, body) = http_req.into_parts();
        let http_req = http::Request::from_parts(parts, body.as_ref());
        let res = agent.run(http_req).map_err(|e| Error::from(Box::new(e)))?;
        let (parts, body_stream) = res.into_parts();
        let mut body = vec![];
        body_stream.into_reader().read_to_end(&mut body)?;
        Ok(satay_runtime::BufferedResponse::new(
            satay_runtime::ResponseParts {
                status: parts.status,
                headers: parts.headers,
                body,
            },
        ))
    }
}

impl<T: satay_runtime::Action> UreqActionExt for T {}

#[cfg(test)]
mod tests;
