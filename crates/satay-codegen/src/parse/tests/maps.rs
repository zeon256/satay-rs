use super::*;

#[test]
fn parses_typed_map_property() {
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    Environment:
      type: object
      required:
        - metadata
      properties:
        metadata:
          type: object
          additionalProperties:
            type: string
"##,
    );

    match &component(&api, "Environment").kind {
        ComponentKind::Struct(fields) => {
            let metadata = field(fields, "metadata");
            assert_eq!(metadata.ty, TypeRef::Map(Box::new(TypeRef::String)));
            assert!(metadata.required);
        }
        other => panic!("expected Environment struct, got {other:?}"),
    }
}

#[test]
fn parses_map_of_component_refs() {
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    ToolConfig:
      type: object
      required:
        - enabled
      properties:
        enabled:
          type: boolean
    Toolset:
      type: object
      properties:
        configs:
          type: object
          additionalProperties:
            $ref: '#/components/schemas/ToolConfig'
"##,
    );

    match &component(&api, "Toolset").kind {
        ComponentKind::Struct(fields) => {
            let configs = field(fields, "configs");
            assert_eq!(
                configs.ty,
                TypeRef::Map(Box::new(TypeRef::Named("ToolConfig".to_owned())))
            );
        }
        other => panic!("expected Toolset struct, got {other:?}"),
    }
}

#[test]
fn parses_freeform_map_property() {
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    OutputFormat:
      type: object
      required:
        - schema
      properties:
        schema:
          type: object
          additionalProperties: true
"##,
    );

    match &component(&api, "OutputFormat").kind {
        ComponentKind::Struct(fields) => {
            let schema = field(fields, "schema");
            assert_eq!(schema.ty, TypeRef::Map(Box::new(TypeRef::JsonValue)));
        }
        other => panic!("expected OutputFormat struct, got {other:?}"),
    }
}

#[test]
fn parses_empty_schema_component_as_json_value_alias() {
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    JsonValue: {}
    Event:
      type: object
      required:
        - payload
      properties:
        payload:
          $ref: '#/components/schemas/JsonValue'
"##,
    );

    match &component(&api, "JsonValue").kind {
        ComponentKind::Alias(alias) => assert_eq!(*alias, TypeRef::JsonValue),
        other => panic!("expected JsonValue alias, got {other:?}"),
    }

    match &component(&api, "Event").kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(field(fields, "payload").ty, TypeRef::JsonValue);
        }
        other => panic!("expected Event struct, got {other:?}"),
    }
}

#[test]
fn parses_array_of_maps_of_empty_schema_refs() {
    // The Anthropic `input_examples` shape: array items are maps whose values
    // reference an empty-schema component.
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    JsonValue: {}
    BashTool:
      type: object
      properties:
        input_examples:
          type: array
          items:
            type: object
            additionalProperties:
              $ref: '#/components/schemas/JsonValue'
"##,
    );

    match &component(&api, "BashTool").kind {
        ComponentKind::Struct(fields) => {
            let input_examples = field(fields, "input_examples");
            assert_eq!(
                input_examples.ty,
                TypeRef::Array(Box::new(TypeRef::Map(Box::new(TypeRef::JsonValue))))
            );
            assert!(!input_examples.required);
        }
        other => panic!("expected BashTool struct, got {other:?}"),
    }
}

#[test]
fn parses_nullable_map_union_wrapper() {
    // The BetaMCPToolset.configs shape: `anyOf: [map, null]` hoists the map
    // out of the wrapper union and renders as an optional map.
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    ToolConfig:
      type: object
      required:
        - enabled
      properties:
        enabled:
          type: boolean
    Toolset:
      type: object
      properties:
        configs:
          anyOf:
            - type: object
              additionalProperties:
                $ref: '#/components/schemas/ToolConfig'
            - type: 'null'
"##,
    );

    match &component(&api, "Toolset").kind {
        ComponentKind::Struct(fields) => {
            let configs = field(fields, "configs");
            assert_eq!(
                configs.ty,
                TypeRef::Option(Box::new(TypeRef::Map(Box::new(TypeRef::Named(
                    "ToolConfig".to_owned()
                )))))
            );
        }
        other => panic!("expected Toolset struct, got {other:?}"),
    }

    assert!(
        !api.components
            .iter()
            .any(|component| component.rust_name == "ToolsetConfigs"),
        "nullable map wrapper must not synthesize a wrapper component"
    );
}

#[test]
fn parses_map_with_nullable_values() {
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    UpdateRequest:
      type: object
      properties:
        metadata:
          type: object
          additionalProperties:
            anyOf:
              - type: string
              - type: 'null'
"##,
    );

    match &component(&api, "UpdateRequest").kind {
        ComponentKind::Struct(fields) => {
            let metadata = field(fields, "metadata");
            // The single inline string branch keeps its untagged wrapper enum
            // (collapsing inline branches is tracked by issue #48); the wire
            // format is a plain nullable string either way.
            assert_eq!(
                metadata.ty,
                TypeRef::Map(Box::new(TypeRef::Option(Box::new(TypeRef::Named(
                    "UpdateRequestMetadataValue".to_owned()
                )))))
            );
        }
        other => panic!("expected UpdateRequest struct, got {other:?}"),
    }

    match &component(&api, "UpdateRequestMetadataValue").kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            assert_eq!(union.variants.len(), 1);
            assert_eq!(union.variants[0].ty, TypeRef::String);
        }
        other => panic!("expected UpdateRequestMetadataValue union, got {other:?}"),
    }
}

#[test]
fn parses_struct_with_additional_properties_sibling() {
    // Structs that also allow extra properties keep generating plain structs;
    // the `additionalProperties` sibling is ignored.
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    InputSchema:
      type: object
      additionalProperties: true
      required:
        - type
      properties:
        type:
          type: string
"##,
    );

    match &component(&api, "InputSchema").kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].wire_name, "type");
        }
        other => panic!("expected InputSchema struct, got {other:?}"),
    }
}

#[test]
fn rejects_object_without_properties_or_additional_properties() {
    let err = parse_invalid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    Holder:
      type: object
      required:
        - value
      properties:
        value:
          type: object
"##,
    );
    match err {
        ValidationError::UnsupportedMapObjectSchema { context } => {
            assert_eq!(context, "property `Holder.value`");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_closed_empty_object_schema() {
    let err = parse_invalid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    Holder:
      type: object
      properties:
        value:
          type: object
          additionalProperties: false
"##,
    );
    match err {
        ValidationError::UnsupportedMapObjectSchema { context } => {
            assert_eq!(context, "property `Holder.value`");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_map_query_parameter() {
    let err = parse_invalid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /search:
    get:
      operationId: search
      parameters:
        - name: filters
          in: query
          schema:
            type: object
            additionalProperties:
              type: string
      responses:
        '204':
          description: No content
"##,
    );
    match err {
        ValidationError::MapParameterUnsupported { wire_name } => {
            assert_eq!(wire_name, "filters");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_map_with_min_properties() {
    let err = parse_invalid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    Holder:
      type: object
      properties:
        metadata:
          type: object
          minProperties: 1
          additionalProperties:
            type: string
"##,
    );
    match err {
        ValidationError::UnsupportedKeyword { context, keyword } => {
            assert_eq!(context, "property `Holder.metadata`");
            assert_eq!(keyword, "minProperties");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn parses_freeform_map_component_as_alias() {
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    Freeform:
      type: object
      additionalProperties: true
    Holder:
      type: object
      required:
        - value
      properties:
        value:
          $ref: '#/components/schemas/Freeform'
"##,
    );

    match &component(&api, "Freeform").kind {
        ComponentKind::Alias(ty) => {
            assert_eq!(ty, &TypeRef::Map(Box::new(TypeRef::JsonValue)));
        }
        other => panic!("expected Freeform alias, got {other:?}"),
    }

    // Alias refs are inlined at lowering; the field carries the map shape.
    match &component(&api, "Holder").kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(
                field(fields, "value").ty,
                TypeRef::Map(Box::new(TypeRef::JsonValue))
            );
        }
        other => panic!("expected Holder struct, got {other:?}"),
    }
}

#[test]
fn parses_typed_map_component() {
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    Labels:
      type: object
      additionalProperties:
        type: string
"##,
    );

    match &component(&api, "Labels").kind {
        ComponentKind::Alias(ty) => {
            assert_eq!(ty, &TypeRef::Map(Box::new(TypeRef::String)));
        }
        other => panic!("expected Labels alias, got {other:?}"),
    }
}

#[test]
fn rejects_propertyless_object_component_without_additional_properties() {
    let err = parse_invalid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    Empty:
      type: object
"##,
    );

    match err {
        ValidationError::UnsupportedMapObjectSchema { context } => {
            assert_eq!(context, "schema `Empty`");
        }
        other => panic!("unexpected error: {other}"),
    }
}
