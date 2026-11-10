use super::*;
use http::header::HeaderName;
use std::borrow::Cow;
use std::fmt;

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

impl StorageAction for BorrowingAction {
    type StorageResponse<'s, S: Storage + 's> = S::Text<'s>;

    fn decode_in<'s, S: Storage<Error: fmt::Display>>(
        response: ResponseParts<&[u8]>,
        storage: &'s S,
    ) -> Result<S::Text<'s>, Error> {
        shape::from_projected_json_slice::<shape::Text, S>(response.body, "name", None, storage)
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

#[test]
fn storage_decoding_copies_into_the_policy_and_releases_the_buffer() {
    let response = BufferedResponse::<BorrowingAction, _>::new(ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"name":"Mochi"}"#.to_vec(),
    });
    let boxed: Box<str> = response.decode_in(&BoxedStorage).unwrap();
    let standard: String = response.decode_in(&AllocStorage).unwrap();
    drop(response);
    assert_eq!((&*boxed, standard.as_str()), ("Mochi", "Mochi"));
}
