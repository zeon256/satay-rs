use super::generated::*;

#[test]
fn wrapped_union_branch_round_trips() {
    let params: Params = serde_json::from_str(r#"{"budget":5}"#).expect("deserialized params");
    match &params {
        Params::AutoParams(auto) => assert_eq!(auto.budget, 5),
        other => panic!("unexpected params: {other:?}"),
    }
    let encoded = serde_json::to_value(&params).expect("serialized params");
    assert_eq!(encoded, serde_json::json!({"budget": 5}));
}

#[test]
fn wrapped_enum_property_round_trips() {
    let profile: Profile =
        serde_json::from_str(r#"{"relationship":"friend"}"#).expect("deserialized profile");
    assert_eq!(profile.relationship, Relationship::Friend);
    let encoded = serde_json::to_value(&profile).expect("serialized profile");
    assert_eq!(encoded, serde_json::json!({"relationship": "friend"}));
}
