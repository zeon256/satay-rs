use super::codegen;
use std::fs;

use super::ast;
use super::common::*;

#[test]
fn generated_storage_is_selected_without_regenerating_models() {
    let files = codegen::generate(
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
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/storage/generated_storage_is_selected_without_regenerating_models/tests.rs"
        ),
    );
    fs::write(temp.path().join("src/lib.rs"), TEST_CRATE_LIB).unwrap();
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
    let files = codegen::generate(
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
    write_fixture_tests(
        temp.path(),
        include_str!("tests/storage/storage_parameter_does_not_shadow_schema_names/tests.rs"),
    );
    fs::write(temp.path().join("src/lib.rs"), TEST_CRATE_LIB).unwrap();
    run_temp_cargo(
        temp.path(),
        "test",
        &[],
        "schema/storage generic name collisions",
    );
}

#[test]
fn lossy_storage_bounds_propagate_through_containing_models() {
    let files = codegen::generate(
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
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/storage/lossy_storage_bounds_propagate_through_containing_models/tests.rs"
        ),
    );
    fs::write(temp.path().join("src/lib.rs"), TEST_CRATE_LIB).unwrap();
    run_temp_cargo(temp.path(), "test", &[], "lossy storage bounds");
    run_temp_cargo(
        temp.path(),
        "check",
        &["--no-default-features"],
        "lossy storage without serde",
    );
}
