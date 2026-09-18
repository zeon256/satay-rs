use super::normalize;
use crate::error::ValidationError;
use crate::parse::normalize::{NormalizeError, normalize_spec};
use crate::parse::tests::parse_valid;

#[test]
fn operationless_paths_retain_parameters_and_shared_paths_drop_only_skipped_operations() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Paths, version: '1'}
paths:
  /empty/{id}:
    parameters:
      - {name: id, in: path, required: true, schema: {type: string}}
  /shared/{id}:
    parameters:
      - {name: id, in: path, required: true, schema: {type: string}}
    get:
      operationId: kept
      responses: {'204': {description: empty}}
    delete:
      operationId: skipped
      x-satay: {skip: true}
  /gone:
    parameters:
      - {name: unsupported, in: cookie, schema: false}
    post:
      x-satay: {skip: true}
"#,
    );

    assert_eq!(
        api.http()
            .paths
            .iter()
            .map(|p| p.path.as_str())
            .collect::<Vec<_>>(),
        ["/empty/{id}", "/shared/{id}"]
    );
    assert!(api.http().paths[0].operations.is_empty());
    assert_eq!(api.http().paths[0].parameters[0].wire_name, "id");
    assert_eq!(api.http().paths[1].parameters[0].wire_name, "id");
    assert_eq!(
        api.http().paths[1]
            .operations
            .iter()
            .map(|o| o.source_id.as_deref())
            .collect::<Vec<_>>(),
        [Some("kept")]
    );
    assert!(!api.http().paths[1].operations[0].interpretation.skip);
}

#[test]
fn unresolved_skipped_reference_precedes_version_selection_and_keeps_document_location() {
    let spec = r#"
openapi: 3.0.0
info: {title: References first, version: '1'}
paths:
  /skipped:
    post:
      x-satay: {skip: true}
      requestBody:
        content:
          multipart/form-data:
            schema: {$ref: '#/components/schemas/Missing'}
      responses: {'204': {description: empty}}
"#;

    let NormalizeError::Validation { location, source } =
        normalize_spec(spec, "refs.yaml").unwrap_err()
    else {
        panic!("reference resolution error")
    };

    assert_eq!(location.document, "refs.yaml");
    assert_eq!(location.pointer, "");
    assert!(
        matches!(source.as_ref(), ValidationError::ResolveReference { reference, .. }
        if reference == "#/components/schemas/Missing")
    );
}

#[test]
fn alternative_media_reference_to_legacy_excluded_definition_errors_explicitly() {
    let spec = r#"
openapi: 3.1.0
info: {title: All media, version: '1'}
paths:
  /active:
    get:
      operationId: active
      responses:
        '200':
          description: result
          content:
            application/json:
              schema: {type: string}
            text/plain:
              schema: {$ref: '#/components/schemas/Skipped'}
  /skip:
    post:
      x-satay: {skip: true}
      requestBody:
        content:
          application/json:
            schema: {$ref: '#/components/schemas/Skipped'}
      responses: {'204': {description: empty}}
components:
  schemas:
    Skipped: {type: string}
"#;

    parse_valid(spec);
    crate::generate(spec).expect("compatibility input generates");

    let NormalizeError::ExcludedDefinition { name, location } =
        normalize_spec(spec, "media.yaml").unwrap_err()
    else {
        panic!("excluded target must not be fabricated or silently included")
    };

    assert_eq!(name, "Skipped");
    assert_eq!(location.document, "media.yaml");
    assert_eq!(
        location.pointer,
        "/paths/~1active/get/responses/200/content/text~1plain/schema"
    );
}
