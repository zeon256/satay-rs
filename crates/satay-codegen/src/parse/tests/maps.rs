use satay_ir::{AdditionalProperties, TypeExpr};

use super::ast::*;
use super::*;
use syn::Fields;

#[test]
fn parses_typed_map_property() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let environment = find_struct(&types, "Environment");
    assert_field(
        environment,
        "metadata",
        "BTreeMap<String, <S as satay_runtime::storage::Storage>::Text<'storage>>",
    );
    // Required properties are not Option-wrapped and carry no serde default.
    let metadata = field(environment, "metadata");
    assert!(
        !metadata
            .attrs
            .iter()
            .any(|attr| norm(attr).contains("serde (default")),
        "required field `Environment.metadata` must not be optional"
    );
}

#[test]
fn parses_map_of_component_refs() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    // The property is not required, so the map is Option-wrapped in the output.
    let toolset = find_struct(&types, "Toolset");
    assert_field(toolset, "configs", "Option<BTreeMap<String, ToolConfig>>");
}

#[test]
fn parses_freeform_map_property() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let output_format = find_struct(&types, "OutputFormat");
    assert_field(
        output_format,
        "schema",
        "BTreeMap<String, satay_runtime::JsonValue>",
    );
}

#[test]
fn parses_empty_schema_component_as_json_value_alias() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let alias = find_type_alias(&types, "JsonValue");
    assert_eq!(norm(&alias.ty), norm_str("satay_runtime::JsonValue"));

    let event = find_struct(&types, "Event");
    assert_field(event, "payload", "satay_runtime::JsonValue");
}

#[test]
fn parses_array_of_maps_of_empty_schema_refs() {
    // The Anthropic `input_examples` shape: array items are maps whose values
    // reference an empty-schema component.
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let bash_tool = find_struct(&types, "BashTool");
    // NOTE: the model's `!required` fact surfaces publicly as the
    // `Option<...>` wrapper plus its serde skip/default attribute.
    assert_field(
        bash_tool,
        "input_examples",
        "Option<<S as satay_runtime::storage::Storage>::Contiguous<'storage, BTreeMap<String, satay_runtime::JsonValue>>>",
    );
    assert_attr_contains(
        &field(bash_tool, "input_examples").attrs,
        "cfg_attr",
        r#"serde(default, skip_serializing_if = "Option::is_none")"#,
    );
}

#[test]
fn parses_nullable_map_union_wrapper() {
    // The BetaMCPToolset.configs shape: `anyOf: [map, null]` hoists the map
    // out of the wrapper union and renders as an optional map.
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let toolset = find_struct(&types, "Toolset");
    assert_field(toolset, "configs", "Option<BTreeMap<String, ToolConfig>>");
    assert!(
        !contains_ident(&types, "ToolsetConfigs"),
        "nullable map wrapper must not synthesize a wrapper component"
    );
}

#[test]
fn parses_map_with_nullable_values() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    // The single inline string branch keeps its untagged wrapper enum
    // (collapsing inline branches is tracked by issue #48); the wire
    // format is a plain nullable string either way.
    let update_request = find_struct(&types, "UpdateRequest");
    assert_field(
        update_request,
        "metadata",
        "Option<BTreeMap<String, Option<UpdateRequestMetadataValue<'storage, S>>>>",
    );

    let union = find_enum(&types, "UpdateRequestMetadataValue");
    assert!(
        union
            .attrs
            .iter()
            .any(|attr| norm(attr).contains(&norm_str("untagged"))),
        "wrapper enum must be untagged"
    );
    assert_eq!(variant_names(union), ["String"]);
    let string_fields = match &variant(union, "String").fields {
        Fields::Unnamed(fields) => fields,
        other => panic!("expected tuple variant, got {}", norm(other)),
    };
    assert_eq!(string_fields.unnamed.len(), 1);
    assert_eq!(
        norm(&string_fields.unnamed[0].ty),
        norm_str("<S as satay_runtime::storage::Storage>::Text<'storage>")
    );
}

#[test]
fn parses_struct_with_additional_properties_sibling() {
    // Structs that also allow extra properties keep generating plain structs;
    // the `additionalProperties` sibling is ignored.
    let spec = r##"
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
"##;
    let files = generate_valid(spec);
    let semantic = normalize_spec(spec);
    let (_, input) = semantic
        .definitions()
        .find(|(_, d)| d.source_name == "InputSchema")
        .unwrap();

    let TypeExpr::Object(input) = &input.schema.ty else {
        panic!("object")
    };

    assert_eq!(input.additional_properties, AdditionalProperties::Allowed);
    assert_eq!(input.properties[0].wire_name, "type");
    assert!(input.properties[0].required);

    let types = parse_rust(file(&files, "types.rs"));
    let input_schema = find_struct(&types, "InputSchema");
    assert_eq!(field_names(input_schema), ["r#type"]);
    assert_field(
        input_schema,
        "r#type",
        "<S as satay_runtime::storage::Storage>::Text<'storage>",
    );
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
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let alias = find_type_alias(&types, "Freeform");
    assert_eq!(
        norm(&alias.ty),
        norm_str("BTreeMap<String, satay_runtime::JsonValue>")
    );

    // Alias refs are inlined at lowering; the field carries the map shape.
    let holder = find_struct(&types, "Holder");
    assert_field(
        holder,
        "value",
        "BTreeMap<String, satay_runtime::JsonValue>",
    );
}

#[test]
fn parses_typed_map_component() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let alias = find_type_alias(&types, "Labels");
    assert_eq!(
        norm(&alias.ty),
        norm_str("BTreeMap<String, <S as satay_runtime::storage::Storage>::Text<'storage>>")
    );
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
