use super::codegen;
use std::fs;

use super::common::*;

#[test]
fn uri_format_round_trips_and_builds_requests() {
    let spec = r#"
openapi: 3.1.0
info:
  title: URL API
  version: 1.0.0
paths:
  /links:
    get:
      operationId: getLinks
      parameters:
        - name: target
          in: query
          required: true
          schema:
            type: string
            format: uri
      responses:
        '204':
          description: No content
components:
  schemas:
    Link:
      type: string
      format: uri
    Links:
      type: object
      required: [url, nullable, urls, byName]
      properties:
        url:
          $ref: '#/components/schemas/Link'
        nullable:
          type: [string, 'null']
          format: uri
        optional:
          type: string
          format: uri
        lenient:
          type: string
          format: uri
          x-satay:
            treat-error-as-none: true
        sentinel:
          type: string
          format: uri
          x-satay:
            none-if: ['']
        urls:
          type: array
          items:
            type: string
            format: uri
        byName:
          type: object
          additionalProperties:
            type: string
            format: uri
"#;
    let files = codegen::generate(spec).expect("generate URL fixture");
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write_manifest(dir, &runtime_path_toml(), false, false);
    write_generated_files(&dir.join("src/generated"), &files);
    write_fixture_tests(
        temp.path(),
        include_str!("tests/urls/uri_format_round_trips_and_builds_requests/tests.rs"),
    );
    fs::write(dir.join("src/lib.rs"), TEST_CRATE_LIB).unwrap();
    run_temp_cargo(dir, "test", &[], "generated URL behavior");
    run_temp_cargo(
        dir,
        "check",
        &["--no-default-features"],
        "URLs without serde",
    );
    // Lossy decoding requires JSON; check serde-only support without that option.
    let serde_spec = spec.replace(
        "          x-satay:\n            treat-error-as-none: true\n",
        "",
    );
    let files = codegen::generate(&serde_spec).unwrap();
    write_generated_files(&dir.join("src/generated"), &files);
    run_temp_cargo(
        dir,
        "check",
        &["--no-default-features", "--features", "serde"],
        "URLs with serde only",
    );
}
