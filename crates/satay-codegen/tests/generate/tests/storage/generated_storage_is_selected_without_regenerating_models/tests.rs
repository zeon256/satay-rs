use super::generated::*;
use compact_str::CompactString;
use satay_runtime::{Action, BufferedResponse, OwnedAction, ResponseParts};

const RECORD: &str = r#"{"name":"Ada","labels":{"team":"ops"},"state":"future","children":[{"label":"kid"}],"choice":{"label":"chosen"}}"#;

#[test]
fn owned_defaults_and_alternate_strings_round_trip() {
    let standard: Record = serde_json::from_str(RECORD).unwrap();
    let boxed: Record<Box<str>> = serde_json::from_str(RECORD).unwrap();
    let compact: Record<CompactString> = serde_json::from_str(RECORD).unwrap();
    assert_eq!(standard.name, boxed.name.as_ref());
    assert_eq!(compact.name.as_str(), "Ada");
    assert_eq!(compact.children[0].label.as_str(), "kid");
    assert_eq!(compact.labels.get("team").unwrap().as_str(), "ops");
    assert!(matches!(&boxed.state, RecordState::Other(value) if value.as_ref() == "future"));
    assert_eq!(compact.state.to_string(), "future");
    assert_eq!(
        serde_json::to_value(&boxed).unwrap(),
        serde_json::to_value(&compact).unwrap()
    );
    let _: Label<Box<str>> = "alias".into();
}

#[test]
fn top_level_map_responses_use_custom_keys_and_values() {
    let buffered = BufferedResponse::<ListLabelsAction<'_, Box<str>>, _>::new(ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"team":"ops"}"#.to_vec(),
    });
    let ListLabelsResponse::Ok(labels) = buffered.decode().unwrap() else {
        panic!("expected labels")
    };
    let (key, value): (&Box<str>, &Box<str>) = labels.first_key_value().unwrap();
    assert_eq!(key.as_ref(), "team");
    assert_eq!(value.as_ref(), "ops");
}

#[test]
fn custom_storage_builders_and_projected_decoding_use_native_buffers() {
    let api = Api::new().string_storage::<CompactString>();
    let record: Record<CompactString> = serde_json::from_str(RECORD).unwrap();
    let action = api.untagged().store_record("a/b", record);
    let request = action.request().unwrap();
    assert_eq!(request.uri(), "/records/a%2Fb");
    assert_eq!(request.headers()["region"], "central");
    assert_eq!(request.headers()["validated"], "ok");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(request.body()).unwrap(),
        serde_json::from_str::<serde_json::Value>(RECORD).unwrap()
    );
    let body = format!("{{\"value\":{RECORD}}}")
        .into_bytes()
        .into_boxed_slice();
    let buffered =
        BufferedResponse::<StoreRecordAction<'_, CompactString>, _>::new(ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body,
        });
    let StoreRecordResponse::Ok(decoded) = buffered.decode().unwrap() else {
        panic!("expected record")
    };
    drop(buffered); // Owned strings do not retain the HTTP body.
    assert_eq!(decoded.name.as_str(), "Ada");
    let _ = <StoreRecordAction<'_, CompactString> as Action>::decode;
    let parts = ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: format!("{{\"value\":{RECORD}}}").into_bytes(),
    };
    let owned: StoreRecordResponse<CompactString> =
        StoreRecordAction::<CompactString>::decode_owned(parts.as_bytes()).unwrap();
    drop(parts);
    let StoreRecordResponse::Ok(owned) = owned else {
        panic!("expected owned record")
    };
    assert_eq!(owned, decoded);
}
