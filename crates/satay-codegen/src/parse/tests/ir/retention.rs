use crate::parse::normalize::normalize_for_rust;
use satay_ir::{CompositionKind, PropertyPolicy, TypeExpr};
use serde_json::Value;

#[test]
fn migration_route_retains_facts_independently_of_rust_output() {
    let api = {
        let source = String::from(
            r#"
openapi: 3.1.0
info: {title: Retention, version: '1'}
servers: [{url: 'https://example.test'}]
tags: [{name: records, description: Retained tag}]
paths:
  /record:
    get:
      operationId: record
      tags: [records]
      x-satay: {output: {unwrap-field: value}}
      responses:
        '200':
          description: envelope
          content:
            application/json:
              schema:
                type: object
                properties:
                  value: {$ref: '#/components/schemas/Record'}
                  ignoredEnvelopeField: {type: string}
components:
  schemas:
    Label: {type: string, format: custom-label, minLength: 1, maxLength: 20}
    Count: {type: integer, minimum: 0, maximum: 100}
    Record:
      type: object
      required: [nullable]
      properties:
        nullable: {type: [string, 'null'], default: null}
        absent: {type: string}
        first: {$ref: '#/components/schemas/Label'}
        second: {$ref: '#/components/schemas/Label', x-satay: {treat-error-as-none: true}}
        hidden: {type: string, x-satay: {ignore: true}}
    Choice: {oneOf: [{type: string}, {type: integer}]}
"#,
        );
        crate::generate(&source).unwrap();
        normalize_for_rust(&source, "retention.yaml").unwrap()
    }; // Source text and frontend state have been dropped.
    let definition = |name| {
        api.definitions()
            .find(|(_, d)| d.source_name == name)
            .unwrap()
    };
    let (label_id, label) = definition("Label");
    assert_eq!(
        label.schema.annotations.format.as_deref(),
        Some("custom-label")
    );
    let TypeExpr::String(label_schema) = &label.schema.ty else {
        panic!("string definition")
    };
    assert_eq!(label_schema.constraints.min_length, Some(1));
    assert_eq!(label_schema.constraints.max_length, Some(20));
    let TypeExpr::Integer(count) = &definition("Count").1.schema.ty else {
        panic!("integer definition")
    };
    let bounds = count.constraints.declared.as_ref().unwrap();
    assert_eq!(bounds.minimum, Some(0.into()));
    assert_eq!(bounds.maximum, Some(100.into()));
    let TypeExpr::Object(record) = &definition("Record").1.schema.ty else {
        panic!("object definition")
    };
    let nullable = &record.properties[0];
    assert!(nullable.required && nullable.value.nullable);
    assert_eq!(nullable.value.annotations.default, Some(Value::Null));
    let absent = &record.properties[1];
    assert!(!absent.required && !absent.value.nullable);
    assert_eq!(absent.value.annotations.default, None);
    for property in &record.properties[2..4] {
        assert_eq!(property.value.ty, TypeExpr::Ref(label_id));
    }
    assert_ne!(record.properties[2].policy, record.properties[3].policy);
    assert_eq!(record.properties[4].policy, PropertyPolicy::Ignored);
    assert!(matches!(record.properties[4].value.ty, TypeExpr::String(_)));
    assert_eq!(
        record.properties[4]
            .value
            .annotations
            .source
            .as_ref()
            .unwrap()
            .pointer,
        "/components/schemas/Record/properties/hidden"
    );
    let TypeExpr::Composition(choice) = &definition("Choice").1.schema.ty else {
        panic!("composition")
    };
    assert_eq!(choice.kind, CompositionKind::OneOf);
    assert!(matches!(choice.branches[0].ty, TypeExpr::String(_)));
    assert!(matches!(choice.branches[1].ty, TypeExpr::Integer(_)));
    assert_eq!(api.http().servers[0].url, "https://example.test");
    assert_eq!(
        api.http().tags[0].description.as_deref(),
        Some("Retained tag")
    );
    let response = &api.http().paths[0].operations[0].responses[0].content[0];
    assert!(matches!(
        response.media.schema.as_ref().unwrap().ty,
        TypeExpr::Object(_)
    ));
    let projection = response.projection.as_ref().unwrap();
    assert!(!projection.unwrap_required);
    assert_eq!(projection.map_required, None);
    assert!(!projection.output.nullable);
    assert!(matches!(
        response.projection.as_ref().unwrap().output.ty,
        TypeExpr::Ref(_)
    ));
}
