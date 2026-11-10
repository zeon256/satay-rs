use super::generated::*;

#[test]
fn map_fields_deserialize() {
    let environment: Environment = serde_json::from_str(
        r#"{
            "metadata": {"region": "us-east-1", "tier": "prod"},
            "config": {"nested": {"enabled": true}, "count": 3},
            "input_examples": [{"command": "ls -la"}]
        }"#,
    )
    .expect("deserialized environment");

    assert_eq!(
        environment.metadata.get("region").map(String::as_str),
        Some("us-east-1")
    );
    let config = environment.config.as_ref().expect("config present");
    assert_eq!(
        config.get("count"),
        Some(&satay_runtime::JsonValue::from(3))
    );
    assert_eq!(
        config
            .get("nested")
            .and_then(|nested| nested.get("enabled")),
        Some(&satay_runtime::JsonValue::Bool(true))
    );
    let examples = environment
        .input_examples
        .as_ref()
        .expect("examples present");
    assert_eq!(
        examples[0].get("command"),
        Some(&satay_runtime::JsonValue::from("ls -la"))
    );
}

#[test]
fn map_fields_serialize() {
    use std::collections::BTreeMap;

    let mut metadata = BTreeMap::new();
    metadata.insert("region".to_owned(), "eu-west-1".to_owned());

    let environment: Environment = Environment {
        metadata,
        config: None,
        input_examples: None,
    };
    let encoded = serde_json::to_value(&environment).expect("serialized environment");
    assert_eq!(
        encoded,
        serde_json::json!({
            "metadata": {"region": "eu-west-1"}
        })
    );
}
