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
    fn send_with(self, agent: &ureq::Agent) -> Result<Self::Response, Error> {
        let http_req = self.request()?;
        let res = agent.run(http_req).map_err(|e| Error::from(Box::new(e)))?;
        let (parts, body_stream) = res.into_parts();
        let mut body = vec![];
        body_stream.into_reader().read_to_end(&mut body)?;
        Ok(Self::decode(satay_runtime::ResponseParts {
            status: parts.status,
            headers: parts.headers,
            body,
        })?)
    }
}

impl<T: satay_runtime::Action> UreqActionExt for T {}
