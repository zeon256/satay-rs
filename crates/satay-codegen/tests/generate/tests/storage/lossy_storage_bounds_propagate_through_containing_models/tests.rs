use super::generated::*;
use compact_str::CompactString;
type CompactStorage = satay_runtime::StringPolicy<CompactString>;

#[test]
fn valid_invalid_and_missing_children_work_with_owned_storage() {
    let valid = r#"{"child":{"name":"Mochi"}}"#;
    let default: Parent = serde_json::from_str(valid).unwrap();
    let boxed: Parent<satay_runtime::storage::BoxedStorage> = serde_json::from_str(valid).unwrap();
    let compact: Parent<CompactStorage> = serde_json::from_str(valid).unwrap();
    assert_eq!(default.child.unwrap().name, "Mochi");
    assert_eq!(boxed.child.unwrap().name.as_ref(), "Mochi");
    assert_eq!(compact.child.unwrap().name.as_str(), "Mochi");
    for input in [r#"{"child":{"name":123}}"#, "{}"] {
        assert!(
            serde_json::from_str::<Parent>(input)
                .unwrap()
                .child
                .is_none()
        );
        assert!(
            serde_json::from_str::<Parent<CompactStorage>>(input)
                .unwrap()
                .child
                .is_none()
        );
    }
    let envelope: Envelope<CompactStorage> = serde_json::from_str(
        r#"{"parents":[{"child":{"name":"Mochi"}},{"child":false}],"choices":{"nested":{"child":{"name":"Kit"}}}}"#,
    ).unwrap();
    assert_eq!(
        envelope.parents[0].child.as_ref().unwrap().name.as_str(),
        "Mochi"
    );
    assert!(envelope.parents[1].child.is_none());
    let wire = serde_json::to_value(envelope).unwrap();
    assert_eq!(wire["choices"]["nested"]["child"]["name"], "Kit");
}

// Unrelated models should still accept a bound for just one input lifetime.
fn decode_child<'de, S>(input: &'de str) -> Child<S>
where
    S: satay_runtime::StaticStorage + satay_runtime::storage_serde::CollectionStorage,
{
    serde_json::from_str(input).unwrap()
}

#[test]
fn ordinary_child_keeps_its_weaker_bound() {
    assert_eq!(decode_child::<satay_runtime::storage::AllocStorage>(r#"{"name":"Mochi"}"#).name, "Mochi");
}
