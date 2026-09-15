use satay_ir::{
    Api, Definition, ObjectSchema, SchemaUse, StringInterpretation, StringSchema, TypeExpr,
};
use serde_json::{Number, Value};

use crate::parse::normalize::normalize_spec;

mod constraints;
mod fixtures;
mod http;
mod interpretation;
mod reachability;
mod schema;

fn normalize(spec: &str) -> Api {
    normalize_spec(spec, "test.yaml").expect("semantic OpenAPI normalization")
}

pub(super) fn assert_selection(spec: &str, definitions: &[&str], operations: &[&str]) {
    let api = normalize(spec);
    assert_eq!(
        api.definitions()
            .map(|(_, d)| d.source_name.as_str())
            .collect::<Vec<_>>(),
        definitions
    );
    assert_eq!(
        api.http()
            .paths
            .iter()
            .flat_map(|path| &path.operations)
            .map(|operation| operation.source_id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        operations
    );
    assert!(
        api.http()
            .paths
            .iter()
            .flat_map(|path| &path.operations)
            .all(|operation| !operation.interpretation.skip)
    );
}

fn definition<'a>(api: &'a Api, name: &str) -> &'a Definition {
    api.definitions()
        .find_map(|(_, definition)| (definition.source_name == name).then_some(definition))
        .unwrap_or_else(|| panic!("missing definition {name}"))
}

fn object(value: &SchemaUse) -> &ObjectSchema {
    match &value.ty {
        TypeExpr::Object(object) => object,
        other => panic!("expected object, got {other:?}"),
    }
}

fn string(value: &SchemaUse) -> &StringSchema {
    match &value.ty {
        TypeExpr::String(string) => string,
        other => panic!("expected string, got {other:?}"),
    }
}

#[test]
fn owned_frontend_end_to_end() {
    let api = {
        let input = String::from(
            r#"
openapi: 3.1.0
info: {title: Owned frontend, version: '1'}
paths:
  /search:
    get:
      operationId: search
      security: []
      x-satay:
        output: {unwrap-field: result}
      responses:
        '200':
          description: result
          content:
            application/json:
              schema:
                type: object
                properties:
                  result: {$ref: '#/components/schemas/Search'}
                  extra: {type: boolean}
components:
  schemas:
    Alias: {$ref: '#/components/schemas/Search'}
    Search:
      type: object
      required: [window, note]
      properties:
        window:
          type: string
          minimum: 1
          maximum: 60
          x-satay: {parse-as: integer-range}
        note: {type: [string, 'null'], default: null}
        shared: {$ref: '#/components/schemas/Alias'}
"#,
        );
        normalize(&input)
    };

    let TypeExpr::Ref(search_id) = definition(&api, "Alias").schema.ty else {
        panic!("alias reference retained")
    };

    assert_eq!(api.definition(search_id).unwrap().source_name, "Search");

    let search = object(&definition(&api, "Search").schema);
    let window = &search.properties[0];

    let StringInterpretation::IntegerRange { bounds, .. } = &string(&window.value).interpretation
    else {
        panic!("range interpretation retained")
    };

    assert_eq!(bounds.minimum.as_ref().unwrap().value, Number::from(1));
    assert_eq!(bounds.maximum.as_ref().unwrap().value, Number::from(60));

    let note = &search.properties[1];
    assert!(note.required && note.value.nullable);
    assert_eq!(note.value.annotations.default, Some(Value::Null));

    let operation = &api.http().paths[0].operations[0];
    assert_eq!(operation.source_id.as_deref(), Some("search"));
    assert_eq!(operation.security, Some(vec![]));

    let response = &operation.responses[0].content[0];
    let envelope = object(response.media.schema.as_ref().unwrap());
    assert_eq!(envelope.properties[1].wire_name, "extra");

    let projected = &response.projection.as_ref().unwrap().output;
    assert_eq!(projected.ty, TypeExpr::Ref(search_id));
    assert!(!projected.nullable);
    assert_eq!(
        projected.annotations.source.as_ref().unwrap().pointer,
        "/paths/~1search/get/responses/200/content/application~1json/schema/properties/result"
    );
}
