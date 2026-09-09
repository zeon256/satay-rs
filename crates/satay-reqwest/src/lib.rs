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
mod tests {
    use super::*;
    use satay_runtime::Error as RuntimeError;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    struct BytesAction(String);

    impl satay_runtime::Action for BytesAction {
        type RequestBody = bytes::Bytes;
        type Response<'de> = &'de [u8];

        fn request(self) -> Result<http::Request<Self::RequestBody>, satay_runtime::Error> {
            Ok(http::Request::builder()
                .method("POST")
                .uri(self.0)
                .body(bytes::Bytes::from_static(b"ping"))?)
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

    impl satay_runtime::OwnedAction for BytesAction {
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
        let task = thread::spawn(move || {
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
        (url, task)
    }

    #[tokio::test]
    async fn async_adapter_sends_bytes_and_lends_native_response_buffer() {
        let (url, server) = server(*b"pong");
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let response = ReqwestActionExt::send_buffered_with(BytesAction(url), &client)
            .await
            .unwrap();
        let decoded = response.decode().unwrap();
        assert_eq!(decoded, b"pong");
        assert_eq!(decoded.as_ptr(), response.parts().body.as_ptr());
        server.join().unwrap();
    }

    #[cfg(feature = "blocking")]
    #[test]
    fn blocking_adapter_sends_bytes_and_lends_native_response_buffer() {
        let (url, server) = server(*b"pong");
        let client = blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let response =
            ReqwestBlockingActionExt::send_buffered_with(BytesAction(url), &client).unwrap();
        let decoded = response.decode().unwrap();
        assert_eq!(decoded, b"pong");
        assert_eq!(decoded.as_ptr(), response.parts().body.as_ptr());
        server.join().unwrap();
    }
    #[tokio::test]
    async fn async_owned_response_and_decode_errors_are_returned_directly() {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        for body in [b"pong", b"bad!"] {
            let (url, server) = server(*body);
            let result = ReqwestActionExt::send_with(BytesAction(url), &client).await;
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
        let buffered = ReqwestActionExt::send_buffered_with(BytesAction(url), &client)
            .await
            .unwrap();
        assert!(buffered.decode().is_err());
        server.join().unwrap();
    }

    #[cfg(feature = "blocking")]
    #[test]
    fn blocking_owned_response_and_decode_errors_are_returned_directly() {
        let client = blocking::Client::builder().no_proxy().build().unwrap();
        for body in [b"pong", b"bad!"] {
            let (url, server) = server(*body);
            let result = ReqwestBlockingActionExt::send_with(BytesAction(url), &client);
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
        let buffered =
            ReqwestBlockingActionExt::send_buffered_with(BytesAction(url), &client).unwrap();
        assert!(buffered.decode().is_err());
        server.join().unwrap();
    }
}
