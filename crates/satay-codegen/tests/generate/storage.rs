use super::codegen;
use std::fs;
use std::path::Path;

use super::ast;
use super::common::*;

const POLICIES: &str = include_str!("tests/storage/policies.rs");
const POLICY_CRATE_LIB: &str =
    "pub mod generated;\npub mod policies;\n\n#[cfg(test)]\nmod tests;\n";

/// Adds the crates the fixture policies wrap. `compact_str` deliberately lacks
/// its Serde feature: a policy's containers implement only the storage contract.
fn write_policy_crate(crate_dir: &Path, constrained: bool, files: &[codegen::GeneratedFile]) {
    write_manifest(crate_dir, &runtime_path_toml(), constrained, false);
    let manifest_path = crate_dir.join("Cargo.toml");
    let mut manifest = fs::read_to_string(&manifest_path).unwrap();
    manifest.push_str(
        "\ncompact_str = \"0.9\"\nbumpalo = { version = \"3.20\", default-features = false, features = [\"collections\"] }\n",
    );
    fs::write(manifest_path, manifest).unwrap();
    write_generated_files(&crate_dir.join("src/generated"), files);
    fs::write(crate_dir.join("src/policies.rs"), POLICIES).unwrap();
    fs::write(crate_dir.join("src/lib.rs"), POLICY_CRATE_LIB).unwrap();
}

#[test]
fn storage_policy_is_selected_without_regenerating_models() {
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
  /grids:
    post:
      operationId: storeGrids
      parameters:
        - name: tag
          in: query
          required: true
          schema:
            type: array
            items: {type: string}
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: array
              items: {$ref: '#/components/schemas/Grid'}
      responses:
        '200':
          description: Stored grids
          content:
            application/json:
              schema:
                type: array
                items: {$ref: '#/components/schemas/Grid'}
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
    Grid:
      type: object
      required: [rows, points]
      properties:
        rows:
          type: array
          items:
            type: array
            items: {type: integer, format: int32}
        points:
          type: array
          items: {$ref: '#/components/schemas/Point'}
    Point:
      type: object
      required: [x]
      properties:
        x: {type: integer, format: int32}
"#,
    )
    .unwrap();

    // Arrays make a model generic even when nothing in it is text.
    let types = ast::parse_rust(find_file(&files, "types.rs"));
    assert_eq!(ast::find_struct(&types, "Grid").generics.params.len(), 2);
    assert!(ast::find_struct(&types, "Point").generics.params.is_empty());

    let temp = tempfile::tempdir().unwrap();
    write_policy_crate(temp.path(), true, &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/storage/storage_policy_is_selected_without_regenerating_models/tests.rs"
        ),
    );
    run_temp_cargo(temp.path(), "test", &[], "storage policies");
    run_temp_cargo(
        temp.path(),
        "check",
        &["--no-default-features"],
        "storage policies without serde",
    );
    run_temp_cargo(
        temp.path(),
        "check",
        &["--no-default-features", "--features", "serde"],
        "storage policies with serde only",
    );

    // A model decoded into an arena must not outlive it.
    fs::write(
        temp.path().join("src/escape.rs"),
        include_str!(
            "tests/storage/storage_policy_is_selected_without_regenerating_models/escape.rs"
        ),
    )
    .unwrap();
    fs::write(
        temp.path().join("src/lib.rs"),
        format!("{POLICY_CRATE_LIB}mod escape;\n"),
    )
    .unwrap();
    let stderr = run_temp_cargo_failure(temp.path(), "check", &[], "arena escape");
    assert!(
        stderr.contains("`arena` does not live long enough"),
        "{stderr}"
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
fn lossy_fields_decode_into_every_storage_policy() {
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
        tags:
          type: array
          items: {type: string}
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
    write_policy_crate(temp.path(), false, &files);
    write_fixture_tests(
        temp.path(),
        include_str!("tests/storage/lossy_fields_decode_into_every_storage_policy/tests.rs"),
    );
    run_temp_cargo(temp.path(), "test", &[], "lossy storage fields");
    run_temp_cargo(
        temp.path(),
        "check",
        &["--no-default-features"],
        "lossy storage fields without serde",
    );
}
