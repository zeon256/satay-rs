use super::generated::*;

#[test]
fn schema_names_and_generic_serde_helpers_round_trip() {
    let model: S<Box<str>> =
        serde_json::from_str(r#"{"text":"hello","nested":{"value":42},"reading":""}"#).unwrap();
    assert_eq!(model.text.as_ref(), "hello");
    assert_eq!(model.nested.value, 42);
    assert_eq!(model.reading, None);
    let wire = serde_json::to_value(&model).unwrap();
    assert_eq!(wire["reading"], "");
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: serde_json::to_vec(&wire).unwrap(),
    };
    let decoded = GetSAction::<Box<str>>::decode(response.as_bytes()).unwrap();
    let GetSResponse::Ok(decoded) = decoded else {
        panic!("expected S model")
    };
    assert_eq!(model, decoded);
}
