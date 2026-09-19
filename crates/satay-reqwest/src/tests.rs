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
    let response = ReqwestBlockingActionExt::send_buffered_with(BytesAction(url), &client).unwrap();
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
    let buffered = ReqwestBlockingActionExt::send_buffered_with(BytesAction(url), &client).unwrap();
    assert!(buffered.decode().is_err());
    server.join().unwrap();
}
