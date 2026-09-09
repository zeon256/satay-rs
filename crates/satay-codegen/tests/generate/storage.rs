use std::fs;

use crate::ast;
use crate::common::*;

#[test]
fn generated_storage_is_selected_without_regenerating_models() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.1.0
info: {title: Storage, version: 1.0.0}
paths:
  /labels:
    get:
      operationId: listLabels
      responses:
        '200':
          description: Labels
          content:
            application/json:
              schema:
                type: object
                additionalProperties: {type: string}
  /records/{key}:
    post:
      operationId: storeRecord
      parameters:
        - name: key
          in: path
          required: true
          schema: {type: string}
        - name: region
          in: header
          schema: {type: string, default: central}
        - name: validated
          in: header
          schema: {type: string, minLength: 1, default: ok}
      requestBody:
        required: true
        content:
          application/json:
            schema: {$ref: '#/components/schemas/Record'}
      x-satay:
        output: {unwrap-field: value}
      responses:
        '200':
          description: Stored record
          content:
            application/json:
              schema:
                type: object
                required: [value]
                properties:
                  value: {$ref: '#/components/schemas/Record'}
components:
  schemas:
    Label: {type: string}
    Record:
      type: object
      required: [name, labels, state, children]
      properties:
        name: {$ref: '#/components/schemas/Label'}
        labels:
          type: object
          additionalProperties: {type: string}
        state:
          anyOf:
            - {type: string}
            - {type: string, enum: [ready]}
        children:
          type: array
          items: {$ref: '#/components/schemas/Child'}
        choice:
          anyOf:
            - {$ref: '#/components/schemas/Child'}
            - {type: integer}
    Child:
      type: object
      required: [label]
      properties:
        label: {type: string}
"#,
    )
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    write_manifest(temp.path(), &runtime_path_toml(), true, false);
    let manifest_path = temp.path().join("Cargo.toml");
    let mut manifest = fs::read_to_string(&manifest_path).unwrap();
    manifest.push_str("\ncompact_str = { version = \"0.9\", features = [\"serde\"] }\n");
    fs::write(manifest_path, manifest).unwrap();
    write_generated_files(&temp.path().join("src/generated"), &files);
    fs::write(temp.path().join("src/lib.rs"), r##"
pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;
    use compact_str::CompactString;
    use satay_runtime::{Action, OwnedAction, BufferedResponse, ResponseParts};

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
        assert_eq!(serde_json::to_value(&boxed).unwrap(), serde_json::to_value(&compact).unwrap());
        let _: Label<Box<str>> = "alias".into();
    }

    #[test]
    fn top_level_map_responses_use_custom_keys_and_values() {
        let buffered = BufferedResponse::<ListLabelsAction<'_, Box<str>>, _>::new(ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"team":"ops"}"#.to_vec(),
        });
        let ListLabelsResponse::Ok(labels) = buffered.decode().unwrap() else { panic!("expected labels") };
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
        assert_eq!(serde_json::from_slice::<serde_json::Value>(request.body()).unwrap(), serde_json::from_str::<serde_json::Value>(RECORD).unwrap());
        let body = format!("{{\"value\":{RECORD}}}").into_bytes().into_boxed_slice();
        let buffered = BufferedResponse::<StoreRecordAction<'_, CompactString>, _>::new(ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body,
        });
        let StoreRecordResponse::Ok(decoded) = buffered.decode().unwrap() else { panic!("expected record") };
        drop(buffered); // Owned strings do not retain the HTTP body.
        assert_eq!(decoded.name.as_str(), "Ada");
        let _ = <StoreRecordAction<'_, CompactString> as Action>::decode;
        let parts = ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: format!("{{\"value\":{RECORD}}}").into_bytes(),
        };
        let owned: StoreRecordResponse<CompactString> = StoreRecordAction::<CompactString>::decode_owned(parts.as_bytes()).unwrap();
        drop(parts);
        let StoreRecordResponse::Ok(owned) = owned else { panic!("expected owned record") };
        assert_eq!(owned, decoded);
    }
}
"##).unwrap();
    run_temp_cargo(temp.path(), "test", &[], "generic string storage");
    run_temp_cargo(
        temp.path(),
        "check",
        &["--no-default-features"],
        "generic storage without serde",
    );
    run_temp_cargo(
        temp.path(),
        "check",
        &["--no-default-features", "--features", "serde"],
        "generic storage with serde only",
    );
}

#[test]
fn storage_parameter_does_not_shadow_schema_names() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.1.0
info: {title: Storage names, version: 1.0.0}
paths:
  /s:
    get:
      operationId: getS
      responses:
        '200':
          description: Value
          content:
            application/json:
              schema: {$ref: '#/components/schemas/S'}
components:
  schemas:
    S:
      type: object
      required: [text, nested, reading]
      properties:
        text: {type: string}
        nested: {$ref: '#/components/schemas/S2'}
        reading:
          type: string
          x-satay:
            parse-as: i32
            none-if: ['']
    S2:
      type: object
      required: [value]
      properties:
        value: {type: integer}
"#,
    )
    .unwrap();
    let types = ast::parse_rust(find_file(&files, "types.rs"));
    let model = ast::find_struct(&types, "S");
    assert_eq!(model.generics.type_params().next().unwrap().ident, "S3");
    let temp = tempfile::tempdir().unwrap();
    write_manifest(temp.path(), &runtime_path_toml(), false, false);
    write_generated_files(&temp.path().join("src/generated"), &files);
    fs::write(
        temp.path().join("src/lib.rs"),
        r##"
pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn schema_names_and_generic_serde_helpers_round_trip() {
        let model: S<Box<str>> = serde_json::from_str(
            r#"{"text":"hello","nested":{"value":42},"reading":""}"#,
        ).unwrap();
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
        let GetSResponse::Ok(decoded) = decoded else { panic!("expected S model") };
        assert_eq!(model, decoded);
    }
}
"##,
    )
    .unwrap();
    run_temp_cargo(
        temp.path(),
        "test",
        &[],
        "schema/storage generic name collisions",
    );
}

#[test]
fn lossy_storage_bounds_propagate_through_containing_models() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.1.0
info: {title: Lossy storage, version: 1.0.0}
paths: {}
components:
  schemas:
    Child:
      type: object
      required: [name]
      properties:
        name: {type: string}
    Parent:
      type: object
      properties:
        child:
          $ref: '#/components/schemas/Child'
          x-satay: {treat-error-as-none: true}
    Parents:
      type: array
      items: {$ref: '#/components/schemas/Parent'}
    Choice:
      anyOf:
        - {$ref: '#/components/schemas/Parent'}
        - {type: integer}
    Envelope:
      type: object
      required: [parents, choices]
      properties:
        parents: {$ref: '#/components/schemas/Parents'}
        choices:
          type: object
          additionalProperties: {$ref: '#/components/schemas/Choice'}
"#,
    )
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    write_manifest(temp.path(), &runtime_path_toml(), false, false);
    let manifest_path = temp.path().join("Cargo.toml");
    let mut manifest = fs::read_to_string(&manifest_path).unwrap();
    manifest.push_str("\ncompact_str = { version = \"0.9\", features = [\"serde\"] }\n");
    fs::write(manifest_path, manifest).unwrap();
    write_generated_files(&temp.path().join("src/generated"), &files);
    fs::write(
        temp.path().join("src/lib.rs"),
        r##"
pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;
    use compact_str::CompactString;

    #[test]
    fn valid_invalid_and_missing_children_work_with_owned_storage() {
        let valid = r#"{"child":{"name":"Mochi"}}"#;
        let default: Parent = serde_json::from_str(valid).unwrap();
        let boxed: Parent<Box<str>> = serde_json::from_str(valid).unwrap();
        let compact: Parent<CompactString> = serde_json::from_str(valid).unwrap();
        assert_eq!(default.child.unwrap().name, "Mochi");
        assert_eq!(boxed.child.unwrap().name.as_ref(), "Mochi");
        assert_eq!(compact.child.unwrap().name.as_str(), "Mochi");
        for input in [r#"{"child":{"name":123}}"#, "{}"] {
            assert!(serde_json::from_str::<Parent>(input).unwrap().child.is_none());
            assert!(serde_json::from_str::<Parent<CompactString>>(input).unwrap().child.is_none());
        }
        let envelope: Envelope<CompactString> = serde_json::from_str(
            r#"{"parents":[{"child":{"name":"Mochi"}},{"child":false}],"choices":{"nested":{"child":{"name":"Kit"}}}}"#,
        ).unwrap();
        assert_eq!(envelope.parents[0].child.as_ref().unwrap().name.as_str(), "Mochi");
        assert!(envelope.parents[1].child.is_none());
        let wire = serde_json::to_value(envelope).unwrap();
        assert_eq!(wire["choices"]["nested"]["child"]["name"], "Kit");
    }

    // Unrelated models should still accept a bound for just one input lifetime.
    fn decode_child<'de, S>(input: &'de str) -> Child<S>
    where
        S: satay_runtime::StringStorage + serde::Deserialize<'de>,
    {
        serde_json::from_str(input).unwrap()
    }

    #[test]
    fn ordinary_child_keeps_its_weaker_bound() {
        assert_eq!(decode_child::<String>(r#"{"name":"Mochi"}"#).name, "Mochi");
    }
}
"##,
    )
    .unwrap();
    run_temp_cargo(temp.path(), "test", &[], "lossy storage bounds");
    run_temp_cargo(
        temp.path(),
        "check",
        &["--no-default-features"],
        "lossy storage without serde",
    );
}
