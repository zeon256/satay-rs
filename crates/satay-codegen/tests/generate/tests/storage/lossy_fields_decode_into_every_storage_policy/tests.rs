use std::fmt;

use satay_runtime::{BoxedStorage, OwnedStorage, from_json_slice_in};

use super::generated::*;
use super::policies::{Arena, Compact};

const ENVELOPE: &str = r#"{"parents":[{"child":{"name":"Mochi"},"tags":["a"]},{"child":false,"tags":[1]}],"choices":{"nested":{"child":{"name":"Kit"}}}}"#;

#[test]
fn valid_invalid_and_missing_values_decode_for_owned_policies() {
    let valid = r#"{"child":{"name":"Mochi"},"tags":["a","b"]}"#;
    let default: Parent = serde_json::from_str(valid).unwrap();
    let boxed: Parent<'_, BoxedStorage> = serde_json::from_str(valid).unwrap();
    assert_eq!(default.child.unwrap().name, "Mochi");
    assert_eq!(default.tags.unwrap(), ["a", "b"]);
    assert_eq!(&*boxed.child.unwrap().name, "Mochi");
    assert_eq!(boxed.tags.unwrap().len(), 2);

    for input in [
        r#"{"child":{"name":123},"tags":"a"}"#,
        r#"{"child":null,"tags":null}"#,
        "{}",
    ] {
        let default: Parent = serde_json::from_str(input).unwrap();
        assert!(default.child.is_none() && default.tags.is_none());
        let compact: Parent<'_, Compact> = serde_json::from_str(input).unwrap();
        assert!(compact.child.is_none() && compact.tags.is_none());
    }
    // Lossy fields forgive a wrong shape, never malformed input.
    assert!(serde_json::from_str::<Parent>(r#"{"child":{"name":}"#).is_err());
}

#[test]
fn lossy_values_decode_into_an_arena_through_containing_models() {
    let arena = Arena::default();
    let envelope: Envelope<'_, Arena> = from_json_slice_in(ENVELOPE.as_bytes(), &arena).unwrap();
    assert_eq!(envelope.parents[0].child.as_ref().unwrap().name, "Mochi");
    assert_eq!(envelope.parents[0].tags.as_ref().unwrap()[0], "a");
    assert!(envelope.parents[1].child.is_none());
    assert!(envelope.parents[1].tags.is_none());
    let wire = serde_json::to_value(&envelope).unwrap();
    assert_eq!(wire["choices"]["nested"]["child"]["name"], "Kit");
    assert_eq!(wire["parents"][1], serde_json::json!({}));
}

// One bound on the policy covers every model reachable from the envelope.
fn decode<S: OwnedStorage<Error: fmt::Display>>(input: &str) -> Envelope<'static, S> {
    serde_json::from_str(input).unwrap()
}

#[test]
fn containing_models_need_only_the_policy_bound() {
    let boxed = decode::<BoxedStorage>(ENVELOPE);
    let compact = decode::<Compact>(ENVELOPE);
    assert_eq!(format!("{boxed:?}"), format!("{compact:?}"));
}
