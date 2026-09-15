use std::collections::HashSet;

use satay_ir::{
    AdditionalProperties, CompositionKind, CompositionSchema, DefinitionId, PropertyPolicy,
    SchemaUse, TypeExpr,
};
use serde_json::{Value, json};

use super::{definition, normalize, object, string};
use crate::error::ValidationError;
use crate::parse::normalize::{NormalizeError, normalize_spec};

fn schema_document(schemas: &str) -> String {
    format!(
        "openapi: 3.1.0\ninfo: {{title: Schema tests, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n{schemas}"
    )
}

fn composition(value: &SchemaUse) -> &CompositionSchema {
    match &value.ty {
        TypeExpr::Composition(composition) => composition,
        other => panic!("expected composition, got {other:?}"),
    }
}

fn reference(value: &SchemaUse) -> DefinitionId {
    match value.ty {
        TypeExpr::Ref(id) => id,
        ref other => panic!("expected reference, got {other:?}"),
    }
}

fn pointer(value: &SchemaUse) -> &str {
    &value.annotations.source.as_ref().unwrap().pointer
}

fn schema_error(schemas: &str) -> (ValidationError, satay_ir::SourceRef) {
    match normalize_spec(&schema_document(schemas), "test.yaml").unwrap_err() {
        NormalizeError::Validation { source, location } => (*source, location),
        other => panic!("expected semantic validation error, got {other:?}"),
    }
}

#[test]
fn forward_shared_alias_and_recursive_identities_use_source_names() {
    let api = {
        let spec = schema_document(
            r##"
    Alias:
      $ref: '#/components/schemas/foo-bar'
    Pair:
      type: object
      properties:
        first: {$ref: '#/components/schemas/foo-bar'}
        again: {$ref: '#/components/schemas/foo-bar'}
        other: {$ref: '#/components/schemas/foo_bar'}
    foo-bar:
      type: object
      properties:
        next: {$ref: '#/components/schemas/foo-bar'}
        mutual: {$ref: '#/components/schemas/foo_bar'}
    foo_bar:
      type: object
      properties:
        previous: {$ref: '#/components/schemas/foo-bar'}
"##,
        );
        normalize(&spec)
    };
    assert_eq!(
        api.definitions()
            .map(|(_, definition)| definition.source_name.as_str())
            .collect::<Vec<_>>(),
        ["Alias", "Pair", "foo-bar", "foo_bar"]
    );
    let pair = object(&definition(&api, "Pair").schema);
    let first = reference(&pair.properties[0].value);
    let other = reference(&pair.properties[2].value);
    assert_eq!(reference(&pair.properties[1].value), first);
    assert_eq!(reference(&definition(&api, "Alias").schema), first);
    assert_ne!(first, other);
    assert_eq!(api.definition(first).unwrap().source_name, "foo-bar");
    assert_eq!(api.definition(other).unwrap().source_name, "foo_bar");
    let alias_id = api
        .definitions()
        .find(|(_, definition)| definition.source_name == "Alias")
        .unwrap()
        .0;
    assert_ne!(alias_id, first);

    // A consumer can traverse the returned recursive graph after every source
    // document and conversion temporary has gone out of scope.
    let mut pending = vec![first];
    let mut visited = HashSet::new();
    let mut names = vec![];
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let definition = api.definition(id).unwrap();
        names.push(definition.source_name.as_str());
        pending.extend(
            object(&definition.schema)
                .properties
                .iter()
                .map(|property| reference(&property.value)),
        );
    }
    names.sort_unstable();
    assert_eq!(names, ["foo-bar", "foo_bar"]);
}

#[test]
fn reference_annotations_and_escaped_sources_are_use_local() {
    let api = normalize(&schema_document(
        r##"
    a/~b:
      type: string
      description: Source description
      format: vendor-code
      default: original
    Alias:
      $ref: '#/components/schemas/a~1~0b'
      description: Alias description
    Record:
      type: object
      properties:
        value/~name:
          $ref: '#/components/schemas/a~1~0b'
        described:
          $ref: '#/components/schemas/a~1~0b'
          description: Local description
"##,
    ));
    let target = &definition(&api, "a/~b").schema;
    assert_eq!(pointer(target), "/components/schemas/a~1~0b");
    assert_eq!(
        target.annotations.source.as_ref().unwrap().document,
        "test.yaml"
    );
    assert_eq!(
        target.annotations.description.as_deref(),
        Some("Source description")
    );
    assert_eq!(target.annotations.default, Some(json!("original")));
    let properties = &object(&definition(&api, "Record").schema).properties;
    let plain = &properties[0].value;
    assert_eq!(
        pointer(plain),
        "/components/schemas/Record/properties/value~1~0name"
    );
    assert_eq!(plain.annotations.description, None);
    assert_eq!(plain.annotations.format, None);
    assert_eq!(plain.annotations.default, None);
    assert_eq!(reference(plain), reference(&properties[1].value));
    assert_eq!(
        properties[1].value.annotations.description.as_deref(),
        Some("Local description")
    );
    let alias = &definition(&api, "Alias").schema;
    assert_eq!(pointer(alias), "/components/schemas/Alias");
    assert_eq!(
        alias.annotations.description.as_deref(),
        Some("Alias description")
    );
    assert_eq!(reference(alias), reference(plain));
}

#[test]
fn object_rules_distinguish_unspecified_boolean_and_typed_schemas() {
    let api = normalize(&schema_document(
        r##"
    Unspecified: {type: object}
    Allowed: {type: object, additionalProperties: true}
    Forbidden: {type: object, additionalProperties: false}
    TypedAny: {type: object, additionalProperties: {}}
    Anything: {}
    Record:
      type: object
      required: [name]
      properties:
        name: {type: string, enum: [small, large]}
        enabled: {type: boolean}
      additionalProperties:
        type: object
        required: [wire]
        properties:
          wire: {type: [string, 'null'], default: null}
"##,
    ));
    assert_eq!(
        object(&definition(&api, "Unspecified").schema).additional_properties,
        AdditionalProperties::Unspecified
    );
    assert_eq!(
        object(&definition(&api, "Allowed").schema).additional_properties,
        AdditionalProperties::Allowed
    );
    assert_eq!(
        object(&definition(&api, "Forbidden").schema).additional_properties,
        AdditionalProperties::Forbidden
    );
    let AdditionalProperties::Schema(any) =
        &object(&definition(&api, "TypedAny").schema).additional_properties
    else {
        panic!("typed empty schema must not become boolean true")
    };
    assert_eq!(any.ty, TypeExpr::AnyJson);
    assert_eq!(
        pointer(any),
        "/components/schemas/TypedAny/additionalProperties"
    );
    assert_eq!(definition(&api, "Anything").schema.ty, TypeExpr::AnyJson);
    let record = object(&definition(&api, "Record").schema);
    assert_eq!(
        record
            .properties
            .iter()
            .map(|property| property.wire_name.as_str())
            .collect::<Vec<_>>(),
        ["name", "enabled"]
    );
    assert!(record.properties[0].required);
    assert!(!record.properties[1].required);
    assert_eq!(record.properties[1].value.ty, TypeExpr::Boolean);
    assert_eq!(
        string(&record.properties[0].value).enum_values.as_deref(),
        Some(["small".to_owned(), "large".to_owned()].as_slice())
    );
    let AdditionalProperties::Schema(additional) = &record.additional_properties else {
        panic!("additional object wire schema retained beside declared properties")
    };
    let field = &object(additional).properties[0];
    assert!(field.required && field.value.nullable);
    assert_eq!(field.value.annotations.default, Some(Value::Null));
    assert_eq!(
        pointer(&field.value),
        "/components/schemas/Record/additionalProperties/properties/wire"
    );
    assert_eq!(
        api.definitions()
            .map(|(_, definition)| definition.source_name.as_str())
            .collect::<Vec<_>>(),
        [
            "Unspecified",
            "Allowed",
            "Forbidden",
            "TypedAny",
            "Anything",
            "Record"
        ]
    );
}

#[test]
fn array_items_preserve_constraints_and_inline_enum_identity() {
    let api = normalize(&schema_document(
        r##"
    Codes:
      type: array
      minItems: 1
      maxItems: 4
      uniqueItems: false
      items:
        type: string
        enum: [one, two]
        minLength: 2
        pattern: '(?=one)'
        format: vendor-code
"##,
    ));
    let TypeExpr::Array(array) = &definition(&api, "Codes").schema.ty else {
        panic!("array retained")
    };
    assert_eq!(
        (array.constraints.min_items, array.constraints.max_items),
        (Some(1), Some(4))
    );
    let items = string(&array.items);
    assert_eq!(
        items.enum_values.as_deref(),
        Some(["one".to_owned(), "two".to_owned()].as_slice())
    );
    assert_eq!(items.constraints.min_length, Some(2));
    assert_eq!(items.constraints.pattern.as_deref(), Some("(?=one)"));
    assert_eq!(
        array.items.annotations.format.as_deref(),
        Some("vendor-code")
    );
    assert_eq!(pointer(&array.items), "/components/schemas/Codes/items");
    assert_eq!(
        api.definitions()
            .map(|(_, definition)| definition.source_name.as_str())
            .collect::<Vec<_>>(),
        ["Codes"]
    );
}

#[test]
fn ignored_inline_records_retain_nested_wire_schemas() {
    let api = normalize(&schema_document(
        r##"
    Record:
      type: object
      required: [legacy]
      properties:
        legacy:
          type: object
          x-satay: {ignore: true}
          additionalProperties: false
          properties:
            batches:
              type: array
              items:
                type: object
                properties:
                  kept: {type: string}
"##,
    ));
    let legacy = &object(&definition(&api, "Record").schema).properties[0];
    assert!(legacy.required);
    assert_eq!(legacy.policy, PropertyPolicy::Ignored);
    let retained = object(&legacy.value);
    assert_eq!(
        retained.additional_properties,
        AdditionalProperties::Forbidden
    );
    let TypeExpr::Array(batches) = &retained.properties[0].value.ty else {
        panic!("array retained")
    };
    assert_eq!(object(&batches.items).properties[0].wire_name, "kept");
    assert_eq!(
        pointer(&object(&batches.items).properties[0].value),
        "/components/schemas/Record/properties/legacy/properties/batches/items/properties/kept"
    );
}

#[test]
fn ordinary_inline_records_still_require_a_supported_boundary() {
    let (error, location) = schema_error(
        r##"
    Record:
      type: object
      properties:
        nested:
          type: object
          properties:
            name: {type: string}
"##,
    );
    assert!(matches!(error, ValidationError::InlineObjectSchema { .. }));
    assert_eq!(
        location.pointer,
        "/components/schemas/Record/properties/nested"
    );
}

#[test]
fn ordered_anyof_oneof_and_open_enum_branches_are_not_lowered_or_hoisted() {
    let api = normalize(&schema_document(
        r##"
    Open:
      description: Keep original branches
      anyOf:
        - type: string
        - type: 'null'
        - type: string
          enum: [one, two]
          const: one
          x-satay:
            enum-variants: {one: raw_variant_name}
    Exclusive:
      oneOf:
        - type: integer
        - type: number
        - type: array
          items: {type: boolean}
    NullableMap:
      anyOf:
        - type: object
          additionalProperties: {type: integer}
        - type: 'null'
"##,
    ));
    let open_use = &definition(&api, "Open").schema;
    let open = composition(open_use);
    assert_eq!(open.kind, CompositionKind::AnyOf);
    assert!(!open_use.nullable);
    assert_eq!(string(&open.branches[0]).enum_values, None);
    assert_eq!(open.branches[1].ty, TypeExpr::Null);
    assert_eq!(
        pointer(&open.branches[1]),
        "/components/schemas/Open/anyOf/1"
    );
    let known = string(&open.branches[2]);
    assert_eq!(
        known.enum_values.as_deref(),
        Some(["one".to_owned(), "two".to_owned()].as_slice())
    );
    assert_eq!(known.const_value.as_deref(), Some("one"));
    assert_eq!(known.enum_variants[0].wire_value, "one");
    assert_eq!(known.enum_variants[0].requested_name, "raw_variant_name");
    let exclusive = composition(&definition(&api, "Exclusive").schema);
    assert_eq!(exclusive.kind, CompositionKind::OneOf);
    assert!(matches!(exclusive.branches[0].ty, TypeExpr::Integer(_)));
    assert!(matches!(exclusive.branches[1].ty, TypeExpr::Number(_)));
    assert!(matches!(exclusive.branches[2].ty, TypeExpr::Array(_)));
    let map = composition(&definition(&api, "NullableMap").schema);
    assert!(matches!(map.branches[0].ty, TypeExpr::Object(_)));
    assert_eq!(map.branches[1].ty, TypeExpr::Null);
}

#[test]
fn allof_branches_and_annotation_wrappers_retain_sources_and_annotations() {
    let api = normalize(&schema_document(
        r##"
    Combined:
      allOf:
        - $ref: '#/components/schemas/Base'
          description: Branch use
        - type: object
          required: [count]
          additionalProperties: false
          properties:
            count: {type: integer}
    Wrapped:
      description: Wrapper description
      title: Wrapper title
      default: null
      allOf:
        - $ref: '#/components/schemas/Text'
    Base:
      type: object
      required: [id]
      properties:
        id: {type: string}
    Text:
      type: string
      description: Original description
"##,
    ));
    let combined = composition(&definition(&api, "Combined").schema);
    assert_eq!(combined.kind, CompositionKind::AllOf);
    assert_eq!(
        api.definition(reference(&combined.branches[0]))
            .unwrap()
            .source_name,
        "Base"
    );
    assert_eq!(
        pointer(&combined.branches[0]),
        "/components/schemas/Combined/allOf/0"
    );
    assert_eq!(
        combined.branches[0].annotations.description.as_deref(),
        Some("Branch use")
    );
    let inline = object(&combined.branches[1]);
    assert_eq!(inline.properties[0].wire_name, "count");
    assert!(inline.properties[0].required);
    assert_eq!(
        inline.additional_properties,
        AdditionalProperties::Forbidden
    );
    assert_eq!(
        pointer(&inline.properties[0].value),
        "/components/schemas/Combined/allOf/1/properties/count"
    );
    let wrapped = &definition(&api, "Wrapped").schema;
    assert_eq!(
        wrapped.annotations.description.as_deref(),
        Some("Wrapper description")
    );
    assert_eq!(wrapped.annotations.default, Some(Value::Null));
    let composition = composition(wrapped);
    assert_eq!(composition.kind, CompositionKind::AllOf);
    assert_eq!(
        api.definition(reference(&composition.branches[0]))
            .unwrap()
            .source_name,
        "Text"
    );
    assert_eq!(composition.branches[0].annotations.description, None);
    assert_eq!(
        pointer(&composition.branches[0]),
        "/components/schemas/Wrapped/allOf/0"
    );
}

#[test]
fn discriminator_mappings_keep_declared_ids_order_and_escaped_locations() {
    let api = normalize(&schema_document(
        r##"
    Event:
      oneOf:
        - $ref: '#/components/schemas/Cat'
        - $ref: '#/components/schemas/Dog'
      discriminator:
        propertyName: kind
        mapping:
          dog/~tag: Dog
          cat-tag: '#/components/schemas/Cat'
    Cat:
      type: object
      required: [kind]
      properties:
        kind: {type: string, enum: [cat-tag, other], const: cat-tag}
        child: {$ref: '#/components/schemas/Cat'}
    Dog:
      allOf:
        - $ref: '#/components/schemas/DogTag'
        - type: object
          properties:
            bark: {type: boolean}
    DogTag:
      type: object
      required: [kind]
      properties:
        kind: {type: string, const: 'dog/~tag'}
"##,
    ));
    let union = composition(&definition(&api, "Event").schema);
    assert_eq!(union.kind, CompositionKind::OneOf);
    let discriminator = union.discriminator.as_ref().unwrap();
    assert_eq!(discriminator.property_name, "kind");
    assert_eq!(
        discriminator
            .mappings
            .iter()
            .map(|mapping| mapping.wire_value.as_str())
            .collect::<Vec<_>>(),
        ["dog/~tag", "cat-tag"]
    );
    assert_eq!(
        discriminator.mappings[0].target,
        reference(&union.branches[1])
    );
    assert_eq!(
        discriminator.mappings[1].target,
        reference(&union.branches[0])
    );
    assert_eq!(
        discriminator.mappings[0].source.as_ref().unwrap().pointer,
        "/components/schemas/Event/discriminator/mapping/dog~1~0tag"
    );
    assert_eq!(
        discriminator.mappings[1].source.as_ref().unwrap().document,
        "test.yaml"
    );
}

#[test]
fn nested_discriminator_and_implicit_tags_stay_declared_not_synthesized() {
    let api = normalize(&schema_document(
        r##"
    Envelope:
      anyOf:
        - oneOf:
            - $ref: '#/components/schemas/Payload'
          discriminator: {propertyName: kind}
        - type: 'null'
    Payload:
      type: object
      properties:
        value: {type: string}
"##,
    ));
    let envelope = composition(&definition(&api, "Envelope").schema);
    let nested = composition(&envelope.branches[0]);
    assert_eq!(nested.kind, CompositionKind::OneOf);
    assert_eq!(nested.discriminator.as_ref().unwrap().property_name, "kind");
    assert!(nested.discriminator.as_ref().unwrap().mappings.is_empty());
    assert_eq!(
        api.definition(reference(&nested.branches[0]))
            .unwrap()
            .source_name,
        "Payload"
    );
    assert_eq!(
        pointer(&nested.branches[0]),
        "/components/schemas/Envelope/anyOf/0/oneOf/0"
    );
    assert_eq!(envelope.branches[1].ty, TypeExpr::Null);
}

#[test]
fn allof_rejects_duplicate_ignored_wire_fields_at_the_second_declaration() {
    let (error, location) = schema_error(
        r##"
    Broken:
      allOf:
        - type: object
          properties:
            duplicate: {type: string, x-satay: {ignore: true}}
        - type: object
          properties:
            duplicate: {type: integer}
"##,
    );
    assert!(
        matches!(error, ValidationError::DuplicateAllOfProperty { property, .. } if property == "duplicate")
    );
    assert_eq!(
        location.pointer,
        "/components/schemas/Broken/allOf/1/properties/duplicate"
    );
}

#[test]
fn allof_rejects_intersections_nested_inline_compositions_and_cycles() {
    let (intersection, location) = schema_error(
        r##"
    Broken:
      allOf:
        - {type: string}
        - {type: integer}
"##,
    );
    assert!(matches!(
        intersection,
        ValidationError::UnsupportedAllOfBranch { index: 0, .. }
    ));
    assert_eq!(location.pointer, "/components/schemas/Broken/allOf/0");
    let (nested, location) = schema_error(
        r##"
    Broken:
      allOf:
        - allOf:
            - {type: object, properties: {id: {type: string}}}
"##,
    );
    assert!(matches!(
        nested,
        ValidationError::UnsupportedAllOfBranch { index: 0, .. }
    ));
    assert_eq!(location.pointer, "/components/schemas/Broken/allOf/0");
    let (cycle, location) = schema_error(
        r##"
    Node:
      type: object
      properties:
        child:
          allOf:
            - $ref: '#/components/schemas/Node'
"##,
    );
    assert!(matches!(cycle, ValidationError::RecursiveAllOf { schema, .. } if schema == "Node"));
    assert_eq!(
        location.pointer,
        "/components/schemas/Node/properties/child/allOf/0"
    );
}

#[test]
fn plain_union_shapes_do_not_admit_unrelated_nested_or_inline_object_support() {
    let (nested, location) = schema_error(
        r##"
    Broken:
      anyOf:
        - oneOf: [{type: string}, {type: integer}]
        - {type: boolean}
"##,
    );
    assert!(matches!(
        nested,
        ValidationError::UnsupportedAnyOfBranch { index: 0, .. }
    ));
    assert_eq!(location.pointer, "/components/schemas/Broken/anyOf/0");
    let (inline, location) = schema_error(
        r##"
    Broken:
      oneOf:
        - type: object
          properties: {id: {type: string}}
        - {type: string}
"##,
    );
    assert!(matches!(
        inline,
        ValidationError::UnsupportedOneOfBranch { index: 0, .. }
    ));
    assert_eq!(location.pointer, "/components/schemas/Broken/oneOf/0");
}

#[test]
fn composition_siblings_and_ref_null_defaults_are_not_silently_erased() {
    let (union, _) = schema_error(
        r##"
    Broken:
      anyOf: [{type: string}, {type: integer}]
      minLength: 1
"##,
    );
    assert!(
        matches!(union, ValidationError::UnsupportedAnyOfSiblingKeyword { keyword, .. } if keyword == "minLength")
    );
    let (allof, _) = schema_error(
        r##"
    Broken:
      allOf: [{type: object, properties: {id: {type: string}}}]
      properties: {other: {type: string}}
"##,
    );
    assert!(
        matches!(allof, ValidationError::UnsupportedAllOfSiblingKeyword { keyword, .. } if keyword == "properties")
    );
    let (reference, location) = schema_error(
        r##"
    Broken: {$ref: '#/components/schemas/Text', default: null}
    Text: {type: string}
"##,
    );
    assert!(
        matches!(reference, ValidationError::UnsupportedRefSiblingKeyword { keyword, .. } if keyword == "default")
    );
    assert_eq!(location.pointer, "/components/schemas/Broken/default");
}

#[test]
fn discriminator_mapping_rejects_nonbranch_duplicate_and_mismatched_targets() {
    let (outside, location) = schema_error(
        r##"
    Broken:
      oneOf: [{$ref: '#/components/schemas/Branch'}]
      discriminator:
        propertyName: kind
        mapping: {bad: Elsewhere}
    Branch: {type: object, properties: {value: {type: string}}}
    Elsewhere: {type: object, properties: {value: {type: string}}}
"##,
    );
    assert!(
        matches!(outside, ValidationError::InvalidDiscriminatorMapping { value, target, .. }
        if value == "bad" && target == "Elsewhere")
    );
    assert_eq!(
        location.pointer,
        "/components/schemas/Broken/discriminator/mapping/bad"
    );
    let (duplicate, location) = schema_error(
        r##"
    Broken:
      oneOf: [{$ref: '#/components/schemas/Branch'}]
      discriminator:
        propertyName: kind
        mapping: {first: Branch, second: Branch}
    Branch: {type: object, properties: {value: {type: string}}}
"##,
    );
    assert!(
        matches!(duplicate, ValidationError::DuplicateDiscriminatorMapping { schema, .. } if schema == "Branch")
    );
    assert_eq!(
        location.pointer,
        "/components/schemas/Broken/discriminator/mapping/second"
    );
    let (mismatch, location) = schema_error(
        r##"
    Broken:
      oneOf: [{$ref: '#/components/schemas/Branch'}]
      discriminator:
        propertyName: kind
        mapping: {wrong: Branch}
    Branch:
      type: object
      required: [kind]
      properties: {kind: {type: string, const: actual}}
"##,
    );
    assert!(
        matches!(mismatch, ValidationError::DiscriminatorMappingValueMismatch { value, actual, .. }
        if value == "wrong" && actual == "actual")
    );
    assert_eq!(
        location.pointer,
        "/components/schemas/Broken/discriminator/mapping/wrong"
    );
}

#[test]
fn discriminator_tag_properties_must_be_required_nonnull_singletons() {
    for tag in [
        "{type: [string, 'null'], const: tag}",
        "{type: string, enum: [one, two]}",
        "{type: string, const: tag, x-satay: {ignore: true}}",
    ] {
        let (error, location) = schema_error(&format!(
            r##"
    Broken:
      oneOf: [{{$ref: '#/components/schemas/Branch'}}]
      discriminator: {{propertyName: kind}}
    Branch:
      type: object
      required: [kind]
      properties: {{kind: {tag}}}
"##
        ));
        assert!(
            matches!(error, ValidationError::InvalidDiscriminatorProperty { schema, property, .. }
            if schema == "Branch" && property == "kind")
        );
        assert_eq!(
            location.pointer,
            "/components/schemas/Branch/properties/kind"
        );
    }
    let (optional, location) = schema_error(
        r##"
    Broken:
      oneOf: [{$ref: '#/components/schemas/Branch'}]
      discriminator: {propertyName: kind}
    Branch:
      type: object
      properties: {kind: {type: string, const: tag}}
"##,
    );
    assert!(matches!(
        optional,
        ValidationError::InvalidDiscriminatorProperty { .. }
    ));
    assert_eq!(
        location.pointer,
        "/components/schemas/Branch/properties/kind"
    );
}

#[test]
fn unknown_and_modeled_unsupported_vocabulary_reports_physical_nodes() {
    let (unknown, location) = schema_error(
        r##"
    Broken:
      type: object
      properties:
        value/~field:
          type: array
          items:
            type: string
            not: {const: forbidden}
"##,
    );
    assert!(
        matches!(unknown, ValidationError::UnsupportedKeyword { keyword, .. } if keyword == "not")
    );
    assert_eq!(
        location.pointer,
        "/components/schemas/Broken/properties/value~1~0field/items/not"
    );
    for (schema, keyword) in [
        (
            "{type: array, prefixItems: [{type: string}], items: {type: string}}",
            "prefixItems",
        ),
        ("{type: number, multipleOf: 2}", "multipleOf"),
        ("{type: object, maxProperties: 2}", "maxProperties"),
    ] {
        let (error, location) = schema_error(&format!("    Broken: {schema}\n"));
        assert!(
            matches!(error, ValidationError::UnsupportedKeyword { keyword: actual, .. } if actual == keyword)
        );
        assert_eq!(
            location.pointer,
            format!("/components/schemas/Broken/{keyword}")
        );
    }
    let (unique, location) =
        schema_error("    Broken: {type: array, uniqueItems: true, items: {type: string}}\n");
    assert!(matches!(
        unique,
        ValidationError::UniqueItemsUnsupported { .. }
    ));
    assert_eq!(location.pointer, "/components/schemas/Broken/uniqueItems");
}

#[test]
fn boolean_missing_items_multitype_and_invalid_enums_remain_typed_errors() {
    let (boolean, location) = schema_error("    Broken: true\n");
    assert!(matches!(
        boolean,
        ValidationError::UnsupportedBooleanSchema { .. }
    ));
    assert_eq!(location.pointer, "/components/schemas/Broken");
    let (items, location) = schema_error("    Broken: {type: array}\n");
    assert!(matches!(items, ValidationError::MissingArrayItems { .. }));
    assert_eq!(location.pointer, "/components/schemas/Broken");
    let (multitype, _) = schema_error("    Broken: {type: [string, integer, 'null']}\n");
    assert!(matches!(
        multitype,
        ValidationError::MultipleNonNullSchemaTypesUnsupported { .. }
    ));
    let (enum_kind, _) = schema_error("    Broken: {type: integer, enum: [1, 2]}\n");
    assert!(
        matches!(enum_kind, ValidationError::UnsupportedEnumType { kind, .. } if kind == "integer")
    );
    let (const_kind, _) = schema_error("    Broken: {const: 42}\n");
    assert!(matches!(
        const_kind,
        ValidationError::NonStringEnumValue { .. }
    ));
    let (membership, _) = schema_error("    Broken: {type: string, enum: [one], const: two}\n");
    assert!(matches!(membership, ValidationError::ConstNotInEnum { .. }));
}
