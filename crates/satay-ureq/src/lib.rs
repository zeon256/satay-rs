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
mod tests {
    use super::*;
    use satay_runtime::Error as RuntimeError;
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    struct BoxedAction(String);

    impl satay_runtime::Action for BoxedAction {
        type RequestBody = Box<[u8]>;
        type Response<'de> = &'de [u8];

        fn request(self) -> Result<http::Request<Self::RequestBody>, satay_runtime::Error> {
            Ok(http::Request::builder()
                .method("POST")
                .uri(self.0)
                .body(Box::from(&b"ping"[..]))?)
        }

        fn decode(
            response: satay_runtime::ResponseParts<&[u8]>,
        ) -> Result<Self::Response<'_>, satay_runtime::Error> {
            assert_eq!(response.headers["x-test"], "preserved");
            if response.body != b"pong" {
                return Err(RuntimeError::InvalidResponse("expected pong"));
            }
            Ok(response.body)
        }
    }

    impl satay_runtime::OwnedAction for BoxedAction {
        type OwnedResponse = Vec<u8>;

        fn decode_owned(
            response: satay_runtime::ResponseParts<&[u8]>,
        ) -> Result<Self::OwnedResponse, satay_runtime::Error> {
            use satay_runtime::Action;
            Ok(Self::decode(response)?.to_vec())
        }
    }

    fn server(body: [u8; 4]) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/test", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = vec![];
            let mut chunk = [0; 1024];
            while !request.ends_with(b"\r\n\r\nping") {
                let count = stream.read(&mut chunk).unwrap();
                assert_ne!(count, 0);
                request.extend_from_slice(&chunk[..count]);
            }
            assert!(request.starts_with(b"POST /test HTTP/1.1\r\n"));
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nx-test: preserved\r\nConnection: close\r\n\r\n").unwrap();
            stream.write_all(&body).unwrap();
        });
        (url, server)
    }

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into()
    }

    #[test]
    fn sends_boxed_body_and_lends_response_buffer() {
        let (url, server) = server(*b"pong");
        let response = BoxedAction(url).send_buffered_with(&agent()).unwrap();
        let decoded = response.decode().unwrap();
        assert_eq!(decoded, b"pong");
        assert_eq!(decoded.as_ptr(), response.parts().body.as_ptr());
        server.join().unwrap();
    }

    #[test]
    fn owned_response_and_decode_errors_are_returned_directly() {
        let agent = agent();
        for body in [b"pong", b"bad!"] {
            let (url, server) = server(*body);
            let result = BoxedAction(url).send_with(&agent);
            if body == b"pong" {
                let decoded: Vec<u8> = result.unwrap();
                assert_eq!(decoded, b"pong");
            } else {
                assert!(matches!(
                    result,
                    Err(Error::Satay(RuntimeError::InvalidResponse("expected pong")))
                ));
            }
            server.join().unwrap();
        }
        let (url, server) = server(*b"bad!");
        let buffered = BoxedAction(url).send_buffered_with(&agent).unwrap();
        assert!(buffered.decode().is_err());
        server.join().unwrap();
    }
}
