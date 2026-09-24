use super::*;
use http::header::HeaderName;
use std::borrow::Cow;

struct BorrowingAction;

#[derive(Debug, serde::Deserialize)]
struct Record<'a> {
    #[serde(borrow)]
    name: Cow<'a, str>,
}

impl Action for BorrowingAction {
    type RequestBody = Box<[u8]>;
    type Response<'de> = Record<'de>;

    fn request(self) -> Result<http::Request<Self::RequestBody>, Error> {
        Ok(http::Request::new(Box::from(&b"{}"[..])))
    }

    fn decode(parts: ResponseParts<&[u8]>) -> Result<Record<'_>, Error> {
        assert_eq!(parts.status, http::StatusCode::OK);
        assert_eq!(parts.headers["x-request-id"], "test");
        from_json_slice(parts.body)
    }
}

impl OwnedAction for BorrowingAction {
    type OwnedResponse = String;

    fn decode_owned(response: ResponseParts<&[u8]>) -> Result<String, Error> {
        Ok(Self::decode(response)?.name.into_owned())
    }
}

#[test]
fn borrows_unescaped_strings_from_a_custom_buffer_and_preserves_parts() {
    let body: Box<[u8]> = Box::from(&br#"{"name":"Mochi"}"#[..]);
    let original_ptr = body.as_ptr();
    let response = BufferedResponse::<BorrowingAction, _>::new(ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::from_iter([(
            HeaderName::from_static("x-request-id"),
            http::HeaderValue::from_static("test"),
        )]),
        body,
    });
    let record = response.decode().unwrap();
    assert!(matches!(record.name, Cow::Borrowed("Mochi")));
    assert_eq!(record.name.as_ptr(), response.parts().body[9..].as_ptr());
    assert!(matches!(response.decode().unwrap().name, Cow::Borrowed(_)));
    drop(record);
    let parts = response.into_parts();
    assert_eq!(parts.body.as_ptr(), original_ptr);
    assert_eq!(parts.headers["x-request-id"], "test");
    let owned = BufferedResponse::<BorrowingAction, _>::new(parts)
        .decode_owned()
        .unwrap();
    assert_eq!(owned, "Mochi");
    let request = BorrowingAction.request().unwrap();
    assert_eq!(&**request.body(), b"{}");
}

#[test]
fn escaped_strings_are_owned_and_invalid_json_is_reported_at_decode() {
    let record: Record<'_> = from_json_slice(br#"{"name":"Mo\u0063hi"}"#).unwrap();
    assert!(matches!(record.name, Cow::Owned(ref value) if value == "Mochi"));
    assert!(from_json_slice::<Record<'_>>(b"invalid").is_err());
}
