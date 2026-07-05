use std::fs;

use crate::ast::*;
use crate::common::*;

const MAP_SCHEMAS: &str = r##"
openapi: 3.1.0
info:
  title: Map API
  version: 1.0.0
paths:
  /environment:
    get:
      operationId: getEnvironment
      responses:
        '200':
          description: Environment
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Environment'
components:
  schemas:
    JsonValue: {}
    Environment:
      type: object
      required:
        - metadata
      properties:
        metadata:
          type: object
          additionalProperties:
            type: string
        config:
          type: object
          additionalProperties: true
        input_examples:
          type: array
          items:
            type: object
            additionalProperties:
              $ref: '#/components/schemas/JsonValue'
"##;

#[test]
fn map_schemas_generate_btree_map_fields() {
    let files = satay_codegen::generate(MAP_SCHEMAS).expect("generate map schema fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    assert!(
        contains_tokens(&types_rs, "use std::collections::BTreeMap"),
        "types.rs imports BTreeMap when map fields are present"
    );
    let environment = find_struct(&types_rs, "Environment");
    assert_field(environment, "metadata", "BTreeMap<String, String>");
    assert_field(
        environment,
        "config",
        "Option<BTreeMap<String, satay_runtime::JsonValue>>",
    );
    // Component alias refs inline their target type.
    assert_field(
        environment,
        "input_examples",
        "Option<Vec<BTreeMap<String, satay_runtime::JsonValue>>>",
    );
    assert!(
        contains_tokens(&types_rs, "pub type JsonValue = satay_runtime::JsonValue"),
        "empty-schema component generates a JsonValue alias"
    );
}

#[test]
fn generated_map_fields_round_trip() {
    let files = satay_codegen::generate(MAP_SCHEMAS).expect("generate map schema runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
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
            config.get("nested").and_then(|nested| nested.get("enabled")),
            Some(&satay_runtime::JsonValue::Bool(true))
        );
        let examples = environment.input_examples.as_ref().expect("examples present");
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

        let environment = Environment {
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
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "map schema generated crate tests");
}
