use satay_ir::{
    Api, CompositionKind, CompositionSchema, ObjectSchema, Property, SchemaUse, TypeExpr,
};

use super::ast::*;
use super::*;
use syn::{Expr, ExprLit, Fields, Item, Lit, Pat, Stmt};

/// Finds a definition by source name.
fn definition<'a>(api: &'a Api, name: &str) -> &'a satay_ir::Definition {
    api.definitions()
        .find(|(_, definition)| definition.source_name == name)
        .map(|(_, definition)| definition)
        .unwrap_or_else(|| panic!("missing definition `{name}`"))
}

/// Composition of a definition's root schema use.
fn composition<'a>(api: &'a Api, name: &str) -> &'a CompositionSchema {
    let TypeExpr::Composition(composition) = &definition(api, name).schema.ty else {
        panic!("expected `{name}` to be a composition");
    };
    composition
}

/// Property of an IR object schema by wire name.
fn property<'a>(object: &'a ObjectSchema, name: &str) -> &'a Property {
    object
        .properties
        .iter()
        .find(|prop| prop.wire_name == name)
        .unwrap_or_else(|| panic!("object has no property `{name}`"))
}

/// Source name of a reference branch's resolved definition.
fn ref_branch_source_name<'a>(api: &'a Api, branch: &SchemaUse) -> &'a str {
    let TypeExpr::Ref(id) = &branch.ty else {
        panic!("expected a reference branch");
    };
    &api.definition(*id).expect("valid reference").source_name
}

/// Tuple-variant payload types of a generated union enum, in declaration order.
fn variant_payload_types(item: &syn::ItemEnum) -> Vec<String> {
    item.variants
        .iter()
        .filter_map(|variant| match &variant.fields {
            Fields::Unnamed(fields) => fields.unnamed.first().map(|field| norm(&field.ty)),
            _ => None,
        })
        .collect()
}

/// Wire names of a generated enum's variants, read from its `as_str` arms.
fn enum_wire_names(file: &syn::File, name: &str) -> Vec<String> {
    let as_str = find_method(file, name, "as_str");
    let Some(Stmt::Expr(Expr::Match(expr), _)) = as_str.block.stmts.last() else {
        panic!("`{name}.as_str` does not end in a match");
    };
    expr.arms
        .iter()
        .filter_map(|arm| {
            let Pat::Path(path) = &arm.pat else {
                return None;
            };
            if path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "Other")
            {
                return None;
            }
            let Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) = &*arm.body
            else {
                return None;
            };
            Some(text.value())
        })
        .collect()
}

/// Asserts a generated enum is a closed set (no `Other` fallback variant).
fn assert_closed_enum(file: &syn::File, name: &str) {
    assert!(
        !variant_names(find_enum(file, name)).contains(&"Other".to_owned()),
        "enum `{name}` unexpectedly has an `Other` fallback variant"
    );
    assert!(
        find_method(file, name, "as_str").sig.constness.is_some(),
        "closed enum `{name}` must use `const fn as_str`"
    );
}

/// Asserts a generated open string enum keeps its `Other` fallback variant.
fn assert_open_string_enum(file: &syn::File, name: &str) {
    assert_eq!(
        variant_names(find_enum(file, name))
            .last()
            .map(String::as_str),
        Some("Other"),
        "open string enum `{name}` must keep its `Other` fallback variant"
    );
    assert!(
        find_method(file, name, "as_str").sig.constness.is_none(),
        "open string enum `{name}` must use a non-`const` `as_str`"
    );
}

/// Whether any struct, enum, or type alias with the name exists in the file.
fn has_item_named(file: &syn::File, name: &str) -> bool {
    file.items.iter().any(|item| match item {
        Item::Struct(inner) => inner.ident == name,
        Item::Enum(inner) => inner.ident == name,
        Item::Type(inner) => inner.ident == name,
        _ => false,
    })
}
#[test]
fn parses_any_of_component_and_inline_refs_into_ir() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /search:
    get:
      operationId: search
      responses:
        '200':
          description: Search results
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/SearchResult'
components:
  schemas:
    User:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Organization:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    SearchResult:
      description: A search result.
      anyOf:
        - $ref: '#/components/schemas/User'
        - $ref: '#/components/schemas/Organization'
    Envelope:
      type: object
      required:
        - item
      properties:
        item:
          anyOf:
            - $ref: '#/components/schemas/Organization'
            - $ref: '#/components/schemas/User'
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let search_result = find_enum(&types, "SearchResult");
    assert_attr_contains(&search_result.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(search_result), ["User", "Organization"]);
    assert_eq!(
        variant_payload_types(search_result),
        [norm_str("User<S>"), norm_str("Organization<S>")]
    );

    let envelope = find_struct(&types, "Envelope");
    assert_eq!(field_names(envelope), ["item"]);
    assert_field(envelope, "item", "EnvelopeItem<S>");

    let envelope_item = find_enum(&types, "EnvelopeItem");
    assert_attr_contains(&envelope_item.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(envelope_item), ["Organization", "User"]);
    assert_eq!(
        variant_payload_types(envelope_item),
        [norm_str("Organization<S>"), norm_str("User<S>")]
    );

    // The operation response body decodes the SearchResult component.
    let parts = parse_rust(file(&files, "search/parts.rs"));
    let response = find_enum(&parts, "SearchResponse");
    assert_eq!(variant_names(response), ["Ok", "UnexpectedStatus"]);
    assert_eq!(
        variant_payload_types(response)[0],
        norm_str("SearchResult<S>")
    );
}

#[test]
fn parses_one_of_component_and_inline_refs_into_ir() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /assistant:
    get:
      operationId: getAssistant
      responses:
        '200':
          description: Assistant
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AssistantObject'
components:
  schemas:
    AssistantToolsCode:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          enum:
            - code_interpreter
    AssistantToolsFileSearch:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          enum:
            - file_search
    AssistantToolsFunction:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          enum:
            - function
    AssistantTool:
      oneOf:
        - $ref: '#/components/schemas/AssistantToolsCode'
        - $ref: '#/components/schemas/AssistantToolsFileSearch'
        - $ref: '#/components/schemas/AssistantToolsFunction'
    AssistantObject:
      type: object
      required:
        - tools
      properties:
        tools:
          type: array
          items:
            oneOf:
              - $ref: '#/components/schemas/AssistantToolsCode'
              - $ref: '#/components/schemas/AssistantToolsFileSearch'
              - $ref: '#/components/schemas/AssistantToolsFunction'
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let assistant_tool = find_enum(&types, "AssistantTool");
    assert_attr_contains(&assistant_tool.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(
        variant_names(assistant_tool),
        [
            "AssistantToolsCode",
            "AssistantToolsFileSearch",
            "AssistantToolsFunction"
        ]
    );
    assert_eq!(
        variant_payload_types(assistant_tool),
        [
            norm_str("AssistantToolsCode"),
            norm_str("AssistantToolsFileSearch"),
            norm_str("AssistantToolsFunction"),
        ]
    );

    let assistant = find_struct(&types, "AssistantObject");
    assert_field(assistant, "tools", "Vec<AssistantObjectToolsItem>");

    let tools_item = find_enum(&types, "AssistantObjectToolsItem");
    assert_attr_contains(&tools_item.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(
        variant_names(tools_item),
        [
            "AssistantToolsCode",
            "AssistantToolsFileSearch",
            "AssistantToolsFunction"
        ]
    );
}

#[test]
fn parses_one_of_with_inline_singleton_string_enum_branch() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /format:
    get:
      operationId: getFormat
      responses:
        '200':
          description: Response format
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AssistantsApiResponseFormatOption'
components:
  schemas:
    ResponseFormatText:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          enum:
            - text
    ResponseFormatJsonObject:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          enum:
            - json_object
    ResponseFormatJsonSchema:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          enum:
            - json_schema
    AssistantsApiResponseFormatOption:
      description: Response format option.
      oneOf:
        - type: string
          description: '`auto` is the default value'
          enum:
            - auto
          x-stainless-const: true
        - $ref: '#/components/schemas/ResponseFormatText'
        - $ref: '#/components/schemas/ResponseFormatJsonObject'
        - $ref: '#/components/schemas/ResponseFormatJsonSchema'
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let format = find_enum(&types, "AssistantsApiResponseFormatOption");
    assert_doc(&format.attrs, "Response format option.");
    assert_attr_contains(&format.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(
        variant_names(format),
        [
            "Auto",
            "ResponseFormatText",
            "ResponseFormatJsonObject",
            "ResponseFormatJsonSchema"
        ]
    );
    assert_eq!(
        variant_payload_types(format),
        [
            norm_str("AssistantsApiResponseFormatOptionAuto"),
            norm_str("ResponseFormatText"),
            norm_str("ResponseFormatJsonObject"),
            norm_str("ResponseFormatJsonSchema"),
        ]
    );

    let auto = find_enum(&types, "AssistantsApiResponseFormatOptionAuto");
    assert_doc(&auto.attrs, "`auto` is the default value");
    assert_eq!(variant_names(auto), ["Auto"]);
    assert_eq!(
        enum_wire_names(&types, "AssistantsApiResponseFormatOptionAuto"),
        ["auto"]
    );
    assert_closed_enum(&types, "AssistantsApiResponseFormatOptionAuto");
}

#[test]
fn parses_one_of_with_inline_multi_value_string_enum_branch() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /tool-choice:
    get:
      operationId: getToolChoice
      responses:
        '200':
          description: Tool choice
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AssistantsApiToolChoiceOption'
components:
  schemas:
    AssistantsNamedToolChoice:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          enum:
            - file_search
            - function
    AssistantsApiToolChoiceOption:
      description: Tool choice option.
      oneOf:
        - type: string
          enum:
            - none
            - auto
            - required
        - $ref: '#/components/schemas/AssistantsNamedToolChoice'
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let option = find_enum(&types, "AssistantsApiToolChoiceOption");
    assert_doc(&option.attrs, "Tool choice option.");
    assert_attr_contains(&option.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(option), ["Enum", "AssistantsNamedToolChoice"]);
    assert_eq!(
        variant_payload_types(option),
        [
            norm_str("AssistantsApiToolChoiceOptionEnum"),
            norm_str("AssistantsNamedToolChoice"),
        ]
    );

    let enum_branch = find_enum(&types, "AssistantsApiToolChoiceOptionEnum");
    assert_eq!(variant_names(enum_branch), ["None", "Auto", "Required"]);
    assert_eq!(
        enum_wire_names(&types, "AssistantsApiToolChoiceOptionEnum"),
        ["none", "auto", "required"]
    );
    assert_closed_enum(&types, "AssistantsApiToolChoiceOptionEnum");
}

#[test]
fn parses_one_of_with_nullable_inline_primitive_branches() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /message:
    get:
      operationId: getMessage
      responses:
        '200':
          description: Message
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Message'
components:
  schemas:
    ContentPart:
      type: object
      required:
        - type
        - text
      properties:
        type:
          type: string
          enum:
            - text
        text:
          type: string
    Message:
      type: object
      properties:
        content:
          oneOf:
            - type: string
            - type: array
              items:
                $ref: '#/components/schemas/ContentPart'
            - type: "null"
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let message = find_struct(&types, "Message");
    // The null branch drops out as an optional field.
    assert_field(message, "content", "Option<MessageContent<S>>");

    let content = find_enum(&types, "MessageContent");
    assert_attr_contains(&content.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(content), ["String", "Array"]);
    assert_eq!(
        variant_payload_types(content),
        [norm_str("S"), norm_str("Vec<ContentPart<S>>")]
    );
}

#[test]
fn rejects_x_satay_on_plain_union_reference_branch() {
    let err = parse_invalid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Value:
      type: object
      properties:
        id:
          type: string
    Broken:
      anyOf:
        - $ref: '#/components/schemas/Value'
          x-satay:
            parse-as: u8
        - type: string
"##,
    );

    assert!(matches!(
        err,
        ValidationError::UnsupportedRefSiblingKeyword { context, keyword }
            if context == "schema `Broken`.anyOf[0]"
                && keyword == "x-satay.parse-as"
    ));
}

#[test]
fn rejects_x_satay_options_on_plain_union_null_branch() {
    for option in ["parse-as: u8", "other: true"] {
        let err = parse_invalid(&format!(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {{}}
components:
  schemas:
    Broken:
      anyOf:
        - type: string
        - type: 'null'
          x-satay:
            {option}
"#
        ));

        assert!(matches!(
            err,
            ValidationError::UnsupportedAnyOfBranch { context, index }
                if context == "schema `Broken`" && index == 1
        ));
    }
}

#[test]
fn parses_any_of_with_inline_primitive_branches() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /value:
    get:
      operationId: getValue
      responses:
        '200':
          description: Primitive value
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/PrimitiveValue'
components:
  schemas:
    PrimitiveValue:
      anyOf:
        - type: string
        - type: integer
        - type: number
        - type: boolean
        - type: array
          items:
            type: string
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let value = find_enum(&types, "PrimitiveValue");
    assert_attr_contains(&value.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(
        variant_names(value),
        ["String", "Integer", "Number", "Boolean", "Array"]
    );
    assert_eq!(
        variant_payload_types(value),
        [
            norm_str("S"),
            norm_str("i64"),
            norm_str("f64"),
            norm_str("bool"),
            norm_str("Vec<S>"),
        ]
    );
}

#[test]
fn parses_any_of_open_string_enum_branch() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /transcription:
    get:
      operationId: getTranscription
      responses:
        '200':
          description: Transcription
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AudioTranscription'
components:
  schemas:
    AudioTranscription:
      type: object
      properties:
        model:
          description: The model to use for transcription.
          anyOf:
            - type: string
            - type: string
              enum:
                - whisper-1
                - gpt-4o-mini-transcribe
                - gpt-4o-transcribe
"#;

    let files = generate_valid(spec);
    let semantic = normalize_spec(spec);
    let transcription_ir = definition(&semantic, "AudioTranscription");

    let TypeExpr::Object(transcription_ir) = &transcription_ir.schema.ty else {
        panic!("transcription object")
    };

    let TypeExpr::Composition(model_ir) = &transcription_ir.properties[0].value.ty else {
        panic!("open enum remains an ordered composition")
    };
    assert_eq!(model_ir.kind, CompositionKind::AnyOf);

    let TypeExpr::String(fallback) = &model_ir.branches[0].ty else {
        panic!("string")
    };

    assert_eq!(fallback.enum_values, None);
    let TypeExpr::String(values) = &model_ir.branches[1].ty else {
        panic!("enum")
    };

    assert_eq!(
        values.enum_values.as_ref().unwrap(),
        &["whisper-1", "gpt-4o-mini-transcribe", "gpt-4o-transcribe"]
    );

    let types = parse_rust(file(&files, "types.rs"));

    let transcription = find_struct(&types, "AudioTranscription");
    assert_field(transcription, "model", "Option<AudioTranscriptionModel<S>>");

    let model = find_enum(&types, "AudioTranscriptionModel");
    assert_doc(&model.attrs, "The model to use for transcription.");
    assert_eq!(
        variant_names(model),
        [
            "Whisper1",
            "Gpt4oMiniTranscribe",
            "Gpt4oTranscribe",
            "Other"
        ]
    );
    assert_eq!(
        enum_wire_names(&types, "AudioTranscriptionModel"),
        ["whisper-1", "gpt-4o-mini-transcribe", "gpt-4o-transcribe"]
    );
    assert_open_string_enum(&types, "AudioTranscriptionModel");
}

#[test]
fn parses_any_of_open_string_enum_with_annotation_only_string_branch() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /model:
    get:
      operationId: getModel
      responses:
        '200':
          description: Model
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Model'
components:
  schemas:
    Model:
      anyOf:
        - type: string
          title: Model identifier
          description: A future model identifier.
          deprecated: false
          example: future-model
        - type: string
          description: Known model identifiers.
          enum:
            - known-model
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let model = find_enum(&types, "Model");
    assert_doc(&model.attrs, "Known model identifiers.");
    assert_eq!(variant_names(model), ["KnownModel", "Other"]);
    assert_eq!(enum_wire_names(&types, "Model"), ["known-model"]);
    assert_open_string_enum(&types, "Model");
}

#[test]
fn parses_any_of_open_string_enum_prefers_outer_description() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /model:
    get:
      operationId: getModel
      responses:
        '200':
          description: Model
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Model'
components:
  schemas:
    Model:
      description: Preferred model identifier.
      anyOf:
        - type: string
        - type: string
          description: Known model identifiers.
          enum:
            - known-model
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let model = find_enum(&types, "Model");
    assert_doc(&model.attrs, "Preferred model identifier.");
    assert_eq!(variant_names(model), ["KnownModel", "Other"]);
    assert_eq!(enum_wire_names(&types, "Model"), ["known-model"]);
    assert_open_string_enum(&types, "Model");
}

#[test]
fn parses_any_of_open_string_enum_with_bare_const_branches() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /model:
    get:
      operationId: getModel
      responses:
        '200':
          description: Model
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Model'
components:
  schemas:
    Model:
      description: The model that will complete your prompt.
      anyOf:
        - type: string
        - const: claude-sonnet-5
          description: Our best model.
          x-stainless-nominal: false
        - const: claude-opus-4-1
          deprecated: true
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let model = find_enum(&types, "Model");
    assert_doc(&model.attrs, "The model that will complete your prompt.");
    assert_eq!(
        variant_names(model),
        ["ClaudeSonnet5", "ClaudeOpus41", "Other"]
    );
    assert_eq!(
        enum_wire_names(&types, "Model"),
        ["claude-sonnet-5", "claude-opus-4-1"]
    );
    assert_open_string_enum(&types, "Model");
}

#[test]
fn parses_any_of_open_string_enum_mixing_enum_and_const_branches() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /model:
    get:
      operationId: getModel
      responses:
        '200':
          description: Model
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Model'
components:
  schemas:
    Model:
      anyOf:
        - type: string
        - type: string
          enum:
            - a
            - b
        - type: string
          const: c
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let model = find_enum(&types, "Model");
    assert_eq!(variant_names(model), ["A", "B", "C", "Other"]);
    assert_eq!(enum_wire_names(&types, "Model"), ["a", "b", "c"]);
    assert_open_string_enum(&types, "Model");
}

#[test]
fn rejects_duplicate_open_string_enum_value_across_branches() {
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
    Model:
      anyOf:
        - type: string
        - type: string
          enum:
            - all
        - const: all
"##,
    );

    match err {
        ValidationError::DuplicateOpenStringEnumValue { context, value } => {
            assert_eq!(context, "schema `Model`");
            assert_eq!(value, "all");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn parses_plain_union_with_overlapping_inline_enum_branches() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /choice:
    get:
      operationId: getChoice
      responses:
        '200':
          description: Choice
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Choice'
components:
  schemas:
    Choice:
      anyOf:
        - type: string
          enum:
            - a
            - b
        - type: string
          enum:
            - b
            - c
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let choice = find_enum(&types, "Choice");
    assert_attr_contains(&choice.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(choice), ["Enum", "Enum_2"]);
    assert_eq!(
        variant_payload_types(choice),
        [norm_str("ChoiceEnum"), norm_str("ChoiceEnum2")]
    );

    let first = find_enum(&types, "ChoiceEnum");
    assert_eq!(enum_wire_names(&types, "ChoiceEnum"), ["a", "b"]);
    assert_eq!(variant_names(first), ["A", "B"]);
    assert_closed_enum(&types, "ChoiceEnum");

    let second = find_enum(&types, "ChoiceEnum2");
    assert_eq!(enum_wire_names(&types, "ChoiceEnum2"), ["b", "c"]);
    assert_eq!(variant_names(second), ["B", "C"]);
    assert_closed_enum(&types, "ChoiceEnum2");
}

#[test]
fn rejects_duplicate_explicit_variant_name_across_open_enum_branches() {
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
    Model:
      anyOf:
        - type: string
        - type: string
          enum:
            - a
          x-satay:
            enum-variants:
              a: Value
        - type: string
          enum:
            - b
          x-satay:
            enum-variants:
              b: Value
"##,
    );
    match err {
        ValidationError::DuplicateSatayEnumVariantName { context, rust_name } => {
            assert_eq!(context, "schema `Model`");
            assert_eq!(rust_name, "Value");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_non_string_const_branch_in_any_of() {
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
    Broken:
      anyOf:
        - type: string
        - const: 5
"##,
    );
    match err {
        ValidationError::UnsupportedAnyOfBranch { context, index } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(index, 1);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn parses_plain_union_with_const_string_branch() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /widget:
    get:
      operationId: getWidget
      responses:
        '200':
          description: Widget
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Wrapper'
components:
  schemas:
    Widget:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Wrapper:
      type: object
      properties:
        keep:
          anyOf:
            - $ref: '#/components/schemas/Widget'
            - type: string
              const: all
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let wrapper = find_struct(&types, "Wrapper");
    assert_field(wrapper, "keep", "Option<WrapperKeep<S>>");

    let keep = find_enum(&types, "WrapperKeep");
    assert_attr_contains(&keep.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(keep), ["Widget", "All"]);
    assert_eq!(
        variant_payload_types(keep),
        [norm_str("Widget<S>"), norm_str("WrapperKeepAll")]
    );

    let all = find_enum(&types, "WrapperKeepAll");
    assert_eq!(variant_names(all), ["All"]);
    assert_eq!(enum_wire_names(&types, "WrapperKeepAll"), ["all"]);
    assert_closed_enum(&types, "WrapperKeepAll");
}

#[test]
fn parses_plain_union_with_bare_const_branch() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /widget:
    get:
      operationId: getWidget
      responses:
        '200':
          description: Widget
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Wrapper'
components:
  schemas:
    Widget:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Wrapper:
      type: object
      properties:
        keep:
          anyOf:
            - $ref: '#/components/schemas/Widget'
            - const: all
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let keep = find_enum(&types, "WrapperKeep");
    assert_attr_contains(&keep.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(keep), ["Widget", "All"]);
    assert_eq!(
        variant_payload_types(keep),
        [norm_str("Widget<S>"), norm_str("WrapperKeepAll")]
    );

    let all = find_enum(&types, "WrapperKeepAll");
    assert_eq!(variant_names(all), ["All"]);
    assert_eq!(enum_wire_names(&types, "WrapperKeepAll"), ["all"]);
    assert_closed_enum(&types, "WrapperKeepAll");
}

#[test]
fn parses_constrained_string_branch_as_plain_union_not_open_enum() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /model:
    get:
      operationId: getModel
      responses:
        '200':
          description: Model
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Model'
components:
  schemas:
    Model:
      anyOf:
        - type: string
          minLength: 12
        - type: string
          enum:
            - known-model
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let model = find_enum(&types, "Model");
    assert_attr_contains(&model.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(model), ["String", "KnownModel"]);
    assert_eq!(
        variant_payload_types(model),
        [norm_str("ModelString"), norm_str("ModelKnownModel")]
    );

    // The constrained branch lowers to its own nutype tuple struct, not to an
    // open string enum.
    assert_tuple_struct(&types, "ModelString", "String");
    let constrained = find_struct(&types, "ModelString");
    assert_attr_contains(&constrained.attrs, "nutype::nutype", "len_char_min = 12");

    let known_model = find_enum(&types, "ModelKnownModel");
    assert_eq!(variant_names(known_model), ["KnownModel"]);
    assert_eq!(enum_wire_names(&types, "ModelKnownModel"), ["known-model"]);
    assert_closed_enum(&types, "ModelKnownModel");
}

#[test]
fn parses_union_schemas_with_vendor_metadata_extensions() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /stream:
    get:
      operationId: stream
      responses:
        '200':
          description: Stream event
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AssistantStreamEvent'
components:
  schemas:
    ThreadStreamEvent:
      type: object
      properties:
        id:
          type: string
    RunStreamEvent:
      type: object
      properties:
        id:
          type: string
    AssistantStreamEvent:
      description: Assistant stream events.
      oneOf:
        - $ref: '#/components/schemas/ThreadStreamEvent'
        - $ref: '#/components/schemas/RunStreamEvent'
      x-oaiMeta:
        name: Assistant stream events
        beta: true
    SearchResult:
      anyOf:
        - $ref: '#/components/schemas/ThreadStreamEvent'
        - $ref: '#/components/schemas/RunStreamEvent'
      x-acmeMeta:
        owner: docs
    TaggedEvent:
      oneOf:
        - $ref: '#/components/schemas/ThreadStreamEvent'
        - $ref: '#/components/schemas/RunStreamEvent'
      discriminator:
        propertyName: event
      x-oaiMeta:
        name: Tagged stream events
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let assistant_stream_event = find_enum(&types, "AssistantStreamEvent");
    assert_doc(&assistant_stream_event.attrs, "Assistant stream events.");
    assert_attr_contains(&assistant_stream_event.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(
        variant_names(assistant_stream_event),
        ["ThreadStreamEvent", "RunStreamEvent"]
    );

    let search_result = find_enum(&types, "SearchResult");
    assert_attr_contains(&search_result.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(
        variant_names(search_result),
        ["ThreadStreamEvent", "RunStreamEvent"]
    );

    // NOTE: the private model's tag metadata is now asserted through the
    // semantic IR; the generated enum carries `serde(tag = "event")` plus
    // per-variant renames for the branch wire values.
    let semantic = normalize_spec(spec);
    let tagged_ir = composition(&semantic, "TaggedEvent");
    let discriminator = tagged_ir
        .discriminator
        .as_ref()
        .expect("declared discriminator");
    assert_eq!(discriminator.property_name, "event");

    let tagged_event = find_enum(&types, "TaggedEvent");
    assert_attr_contains(&tagged_event.attrs, "cfg_attr", r#"serde(tag = "event")"#);
    assert_eq!(
        variant_names(tagged_event),
        ["ThreadStreamEvent", "RunStreamEvent"]
    );
    assert_attr_contains(
        &variant(tagged_event, "ThreadStreamEvent").attrs,
        "cfg_attr",
        r#"serde(rename = "ThreadStreamEvent")"#,
    );
    assert_attr_contains(
        &variant(tagged_event, "RunStreamEvent").attrs,
        "cfg_attr",
        r#"serde(rename = "RunStreamEvent")"#,
    );
}

#[test]
fn parses_discriminator_with_embedded_singleton_type_fields_into_ir() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Tool API
  version: 1.0.0
paths:
  /tool:
    get:
      operationId: getTool
      responses:
        '200':
          description: Tool call
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ToolCall'
components:
  schemas:
    FunctionToolCall:
      type: object
      required:
        - id
        - type
        - function
      properties:
        id:
          type: string
        type:
          type: string
          enum:
            - function
        function:
          type: string
    CustomToolCall:
      type: object
      required:
        - id
        - type
        - custom
      properties:
        id:
          type: string
        type:
          type: string
          enum:
            - custom
        custom:
          type: string
    ToolCall:
      oneOf:
        - $ref: '#/components/schemas/FunctionToolCall'
        - $ref: '#/components/schemas/CustomToolCall'
      discriminator:
        propertyName: type
        mapping:
          function: '#/components/schemas/FunctionToolCall'
          custom: CustomToolCall
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let function_tool_call = find_struct(&types, "FunctionToolCall");
    assert_eq!(
        field_names(function_tool_call),
        ["id", "r#type", "function"]
    );

    let tool_call = find_enum(&types, "ToolCall");
    // Embedded discriminator: branches carry the tag property as a singleton
    // field, so the generated enum is untagged without per-variant renames.
    assert_attr_contains(&tool_call.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(
        variant_names(tool_call),
        ["FunctionToolCall", "CustomToolCall"]
    );
    assert_eq!(
        variant_payload_types(tool_call),
        [
            norm_str("FunctionToolCall<S>"),
            norm_str("CustomToolCall<S>")
        ]
    );

    // NOTE: the tag metadata (property name and explicit mappings) is asserted
    // through the semantic IR.
    let semantic = normalize_spec(spec);
    let tool_call_ir = composition(&semantic, "ToolCall");
    let discriminator = tool_call_ir
        .discriminator
        .as_ref()
        .expect("embedded discriminator tag");
    assert_eq!(discriminator.property_name, "type");
    assert_eq!(
        discriminator
            .mappings
            .iter()
            .map(|mapping| mapping.wire_value.as_str())
            .collect::<Vec<_>>(),
        ["function", "custom"]
    );
    assert_eq!(
        ref_branch_source_name(&semantic, &tool_call_ir.branches[0]),
        "FunctionToolCall"
    );
    assert_eq!(
        ref_branch_source_name(&semantic, &tool_call_ir.branches[1]),
        "CustomToolCall"
    );
}

#[test]
fn parses_discriminator_with_const_embedded_type_fields_into_ir() {
    let spec = r#"
openapi: 3.1.0
info:
  title: Tool API
  version: 1.0.0
paths:
  /tool:
    get:
      operationId: getTool
      responses:
        '200':
          description: Tool call
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ToolCall'
components:
  schemas:
    FunctionToolCall:
      type: object
      required:
        - id
        - type
        - function
      properties:
        id:
          type: string
        type:
          const: function
        function:
          type: string
    CustomToolCall:
      type: object
      required:
        - id
        - type
        - custom
      properties:
        id:
          type: string
        type:
          type: string
          const: custom
          title: Type
          default: custom
        custom:
          type: string
    ToolCall:
      oneOf:
        - $ref: '#/components/schemas/FunctionToolCall'
        - $ref: '#/components/schemas/CustomToolCall'
      discriminator:
        propertyName: type
        mapping:
          function: '#/components/schemas/FunctionToolCall'
          custom: CustomToolCall
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let function_tool_call = find_struct(&types, "FunctionToolCall");
    assert_eq!(
        field_names(function_tool_call),
        ["id", "r#type", "function"]
    );
    assert_field(function_tool_call, "r#type", "FunctionToolCallType");

    let tool_call = find_enum(&types, "ToolCall");
    // Embedded discriminator: branches carry the tag property as a singleton
    // const field, so the generated enum is untagged without renames.
    assert_attr_contains(&tool_call.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(
        variant_names(tool_call),
        ["FunctionToolCall", "CustomToolCall"]
    );
    assert_eq!(
        variant_payload_types(tool_call),
        [
            norm_str("FunctionToolCall<S>"),
            norm_str("CustomToolCall<S>")
        ]
    );

    // NOTE: the tag metadata (property name and explicit mappings) is asserted
    // through the semantic IR.
    let semantic = normalize_spec(spec);
    let tool_call_ir = composition(&semantic, "ToolCall");
    let discriminator = tool_call_ir
        .discriminator
        .as_ref()
        .expect("embedded discriminator tag");
    assert_eq!(discriminator.property_name, "type");
    assert_eq!(
        discriminator
            .mappings
            .iter()
            .map(|mapping| mapping.wire_value.as_str())
            .collect::<Vec<_>>(),
        ["function", "custom"]
    );
}

#[test]
fn parses_discriminator_with_mixed_const_and_enum_embedded_tags() {
    let spec = r#"
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
    Dog:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          enum:
            - dog
        name:
          type: string
    Cat:
      type: object
      required:
        - kind
      properties:
        kind:
          const: cat
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          dog: Dog
          cat: Cat
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let pet = find_enum(&types, "Pet");
    assert_attr_contains(&pet.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(pet), ["Dog", "Cat"]);
    assert_eq!(
        variant_payload_types(pet),
        [norm_str("Dog<S>"), norm_str("Cat<S>")]
    );

    // Branches embed the `kind` tag property as singleton fields.
    let dog = find_struct(&types, "Dog");
    let cat = find_struct(&types, "Cat");
    assert_field(dog, "kind", "DogKind");
    assert_field(cat, "kind", "CatKind");

    // NOTE: the tag metadata is asserted through the semantic IR.
    let semantic = normalize_spec(spec);
    let pet_ir = composition(&semantic, "Pet");
    let discriminator = pet_ir
        .discriminator
        .as_ref()
        .expect("embedded discriminator tag");
    assert_eq!(discriminator.property_name, "kind");
    assert_eq!(
        discriminator
            .mappings
            .iter()
            .map(|mapping| mapping.wire_value.as_str())
            .collect::<Vec<_>>(),
        ["dog", "cat"]
    );
}

#[test]
fn parses_discriminator_with_const_matching_singleton_enum_tag() {
    let spec = r#"
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
    ViewCommand:
      type: object
      required:
        - command
      properties:
        command:
          type: string
          enum:
            - view
          const: view
    Command:
      oneOf:
        - $ref: '#/components/schemas/ViewCommand'
      discriminator:
        propertyName: command
        mapping:
          view: ViewCommand
"#;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let command = find_enum(&types, "Command");
    assert_attr_contains(&command.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(command), ["ViewCommand"]);
    assert_eq!(variant_payload_types(command), [norm_str("ViewCommand")]);

    let view_command = find_struct(&types, "ViewCommand");
    assert_field(view_command, "command", "ViewCommandCommand");

    // NOTE: the tag metadata is asserted through the semantic IR.
    let semantic = normalize_spec(spec);
    let command_ir = composition(&semantic, "Command");
    let discriminator = command_ir
        .discriminator
        .as_ref()
        .expect("embedded discriminator tag");
    assert_eq!(discriminator.property_name, "command");
    assert_eq!(
        ref_branch_source_name(&semantic, &command_ir.branches[0]),
        "ViewCommand"
    );
}

#[test]
fn parses_empty_any_of_as_json_value() {
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
    Broken:
      anyOf: []
"##;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let broken = find_type_alias(&types, "Broken");
    assert_eq!(norm(&broken.ty), norm_str("satay_runtime::JsonValue"));
}

#[test]
fn rejects_any_of_with_duplicate_null_branch() {
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
    Broken:
      anyOf:
        - type: string
        - type: "null"
        - type: "null"
"##,
    );
    match err {
        ValidationError::DuplicateUnionNullBranch {
            context,
            keyword,
            index,
        } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "anyOf");
            assert_eq!(index, 2);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_one_of_with_only_null_branch() {
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
    Broken:
      oneOf:
        - type: "null"
"##,
    );
    match err {
        ValidationError::NullableUnionWithoutVariants { context, keyword } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "oneOf");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_one_of_with_unconstrained_string_shadowing_string_enum_branch() {
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
    Broken:
      oneOf:
        - type: string
        - type: string
          enum:
            - auto
            - none
"##,
    );
    match err {
        ValidationError::ShadowedUnionBranch {
            context,
            keyword,
            index,
            shadowed_by,
        } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "oneOf");
            assert_eq!(index, 1);
            assert_eq!(shadowed_by, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_any_of_with_constrained_string_shadowing_string_enum_branch() {
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
    Broken:
      anyOf:
        - type: string
          minLength: 1
        - type: string
          enum:
            - known-model
"##,
    );
    match err {
        ValidationError::ShadowedUnionBranch {
            context,
            keyword,
            index,
            shadowed_by,
        } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "anyOf");
            assert_eq!(index, 1);
            assert_eq!(shadowed_by, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_one_of_with_number_shadowing_integer_branch() {
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
    Broken:
      oneOf:
        - type: number
        - type: integer
"##,
    );
    match err {
        ValidationError::ShadowedUnionBranch {
            context,
            keyword,
            index,
            shadowed_by,
        } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "oneOf");
            assert_eq!(index, 1);
            assert_eq!(shadowed_by, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_one_of_with_duplicate_boolean_branch() {
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
    Broken:
      oneOf:
        - type: boolean
        - type: boolean
"##,
    );
    match err {
        ValidationError::ShadowedUnionBranch {
            context,
            keyword,
            index,
            shadowed_by,
        } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "oneOf");
            assert_eq!(index, 1);
            assert_eq!(shadowed_by, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_one_of_with_inline_object_branch() {
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
    Broken:
      oneOf:
        - type: object
          properties:
            id:
              type: string
        - type: string
"##,
    );
    match err {
        ValidationError::UnsupportedOneOfBranch { context, index } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_any_of_with_sibling_type_keyword() {
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
    User:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Broken:
      type: object
      anyOf:
        - $ref: '#/components/schemas/User'
"##,
    );
    match err {
        ValidationError::UnsupportedAnyOfSiblingKeyword { context, keyword } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "type");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_one_of_with_sibling_type_keyword() {
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
    User:
      type: object
      properties:
        id:
          type: string
    Broken:
      type: object
      oneOf:
        - $ref: '#/components/schemas/User'
"##,
    );
    match err {
        ValidationError::UnsupportedOneOfSiblingKeyword { context, keyword } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "type");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_satay_extension_on_plain_union_schema() {
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
    User:
      type: object
      properties:
        id:
          type: string
    Broken:
      oneOf:
        - $ref: '#/components/schemas/User'
      x-satay:
        enum-variants: {}
"##,
    );
    match err {
        ValidationError::UnsupportedOneOfSiblingKeyword { context, keyword } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "x-satay");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_unknown_x_satay_option_on_discriminator_reference_branch() {
    let err = parse_invalid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Dog:
      type: object
      properties:
        name:
          type: string
    Cat:
      type: object
      properties:
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
          x-satay:
            other: true
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
"##,
    );

    assert!(matches!(
        err,
        ValidationError::InvalidExtension { context, path, .. }
            if context == "schema `Pet`.oneOf[0]" && path == "x-satay.other"
    ));
}

#[test]
fn rejects_discriminator_with_inline_branch() {
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
    Broken:
      anyOf:
        - type: object
          properties:
            id:
              type: string
      discriminator:
        propertyName: kind
"##,
    );
    match err {
        ValidationError::UnsupportedDiscriminatorBranch {
            context,
            keyword,
            index,
        } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "anyOf");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_discriminator_branch_that_is_not_an_object() {
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
    Dog:
      type: string
    Cat:
      type: object
      properties:
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
"##,
    );
    match err {
        ValidationError::DiscriminatorBranchNotObject { context, schema } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Dog");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_discriminator_embedded_property_missing_from_some_branches() {
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
    Dog:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          enum:
            - dog
        name:
          type: string
    Cat:
      type: object
      properties:
        name:
          type: string
    Pet:
      anyOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
"##,
    );
    match err {
        ValidationError::InvalidDiscriminatorProperty {
            context,
            schema,
            property,
            expected,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Cat");
            assert_eq!(property, "kind");
            assert_eq!(
                expected,
                "present on every branch when any branch contains it"
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_optional_discriminator_embedded_property() {
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
    Dog:
      type: object
      properties:
        kind:
          type: string
          enum:
            - dog
        name:
          type: string
    Cat:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          enum:
            - cat
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
"##,
    );
    match err {
        ValidationError::InvalidDiscriminatorProperty {
            context,
            schema,
            property,
            expected,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Dog");
            assert_eq!(property, "kind");
            assert_eq!(
                expected,
                "a strict, required, non-null singleton string enum or string const"
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_discriminator_mapping_that_disagrees_with_embedded_value() {
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
    Dog:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          enum:
            - dog
        name:
          type: string
    Cat:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          enum:
            - cat
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          hound: Dog
          cat: Cat
"##,
    );
    match err {
        ValidationError::DiscriminatorMappingValueMismatch {
            context,
            schema,
            value,
            actual,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Dog");
            assert_eq!(value, "hound");
            assert_eq!(actual, "dog");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_discriminator_mapping_that_disagrees_with_const_embedded_value() {
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
    Dog:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          const: dog
        name:
          type: string
    Cat:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          const: cat
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          hound: Dog
          cat: Cat
"##,
    );
    match err {
        ValidationError::DiscriminatorMappingValueMismatch {
            context,
            schema,
            value,
            actual,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Dog");
            assert_eq!(value, "hound");
            assert_eq!(actual, "dog");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_duplicate_const_embedded_discriminator_values() {
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
    Dog:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          const: dog
    Hound:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          const: dog
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Hound'
      discriminator:
        propertyName: kind
"##,
    );
    match err {
        ValidationError::DuplicateDiscriminatorValue { context, value } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(value, "dog");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_non_string_const_embedded_discriminator_property() {
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
    Dog:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          const: 5
    Cat:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          const: cat
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
"##,
    );
    match err {
        ValidationError::InvalidDiscriminatorProperty {
            context,
            schema,
            property,
            expected,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Dog");
            assert_eq!(property, "kind");
            assert_eq!(
                expected,
                "a strict, required, non-null singleton string enum or string const"
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_nullable_const_embedded_discriminator_property() {
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
    Dog:
      type: object
      required:
        - kind
      properties:
        kind:
          type:
            - string
            - 'null'
          const: dog
    Cat:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          const: cat
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
"##,
    );
    match err {
        ValidationError::InvalidDiscriminatorProperty {
            context,
            schema,
            property,
            expected,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Dog");
            assert_eq!(property, "kind");
            assert_eq!(
                expected,
                "a strict, required, non-null singleton string enum or string const"
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_optional_const_embedded_discriminator_property() {
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
    Dog:
      type: object
      properties:
        kind:
          type: string
          const: dog
    Cat:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          const: cat
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
"##,
    );
    match err {
        ValidationError::InvalidDiscriminatorProperty {
            context,
            schema,
            property,
            expected,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Dog");
            assert_eq!(property, "kind");
            assert_eq!(
                expected,
                "a strict, required, non-null singleton string enum or string const"
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_lossy_embedded_discriminator_property() {
    let err = parse_invalid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Dog:
      type: object
      required: [kind]
      properties:
        kind:
          type: string
          const: dog
          x-satay:
            treat-error-as-none: true
    Cat:
      type: object
      required: [kind]
      properties:
        kind:
          type: string
          const: cat
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
"##,
    );

    match err {
        ValidationError::InvalidDiscriminatorProperty {
            context,
            schema,
            property,
            expected,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Dog");
            assert_eq!(property, "kind");
            assert_eq!(
                expected,
                "a strict, required, non-null singleton string enum or string const"
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_const_value_outside_enum() {
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
    Task:
      type: object
      properties:
        status:
          type: string
          enum:
            - open
            - closed
          const: archived
"##,
    );
    match err {
        ValidationError::ConstNotInEnum { context } => {
            assert_eq!(context, "property `Task.status`");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_discriminator_mapping_to_external_url() {
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
    Dog:
      type: object
      properties:
        name:
          type: string
    Cat:
      type: object
      properties:
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          dog: https://example.test/schemas/Dog
          cat: Cat
"##,
    );
    match err {
        ValidationError::InvalidDiscriminatorMapping {
            context,
            value,
            target,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(value, "dog");
            assert_eq!(target, "https://example.test/schemas/Dog");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_discriminator_mapping_target_outside_union() {
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
    Dog:
      type: object
      properties:
        name:
          type: string
    Cat:
      type: object
      properties:
        name:
          type: string
    Wolf:
      type: object
      properties:
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          dog: '#/components/schemas/Wolf'
          cat: Cat
"##,
    );
    match err {
        ValidationError::InvalidDiscriminatorMapping {
            context,
            value,
            target,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(value, "dog");
            assert_eq!(target, "#/components/schemas/Wolf");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_duplicate_discriminator_mapping_targets() {
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
    Dog:
      type: object
      properties:
        name:
          type: string
    Cat:
      type: object
      properties:
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          dog: Dog
          hound: '#/components/schemas/Dog'
          cat: Cat
"##,
    );
    match err {
        ValidationError::DuplicateDiscriminatorMapping { context, schema } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Dog");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_duplicate_discriminator_values() {
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
    Dog:
      type: object
      properties:
        name:
          type: string
    Cat:
      type: object
      properties:
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          Cat: Dog
"##,
    );
    match err {
        ValidationError::DuplicateDiscriminatorValue { context, value } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(value, "Cat");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_recursive_discriminator_union() {
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
    Dog:
      type: object
      properties:
        friend:
          $ref: '#/components/schemas/Pet'
    Cat:
      type: object
      properties:
        name:
          type: string
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          dog: Dog
"##,
    );
    match err {
        ValidationError::RecursiveAnyOf { context, schema } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Pet");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_inline_any_of_parameter_schemas() {
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
      parameters:
        - name: filter
          in: query
          schema:
            anyOf:
              - $ref: '#/components/schemas/User'
              - $ref: '#/components/schemas/Organization'
      responses:
        '204':
          description: No content
components:
  schemas:
    User:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Organization:
      type: object
      required:
        - id
      properties:
        id:
          type: string
"##,
    );
    match err {
        ValidationError::AnyOfParameterUnsupported { wire_name } => {
            assert_eq!(wire_name, "filter");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_parameters_referencing_any_of_components() {
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
      parameters:
        - name: filter
          in: query
          schema:
            $ref: '#/components/schemas/SearchResult'
      responses:
        '204':
          description: No content
components:
  schemas:
    User:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Organization:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    SearchResult:
      anyOf:
        - $ref: '#/components/schemas/User'
        - $ref: '#/components/schemas/Organization'
"##,
    );
    match err {
        ValidationError::AnyOfParameterUnsupported { wire_name } => {
            assert_eq!(wire_name, "filter");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_mutually_recursive_any_of_components() {
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
    A:
      anyOf:
        - $ref: '#/components/schemas/B'
    B:
      anyOf:
        - $ref: '#/components/schemas/A'
"##,
    );
    match err {
        ValidationError::RecursiveAnyOf { context, schema } => {
            assert_eq!(context, "schema `A`");
            assert_eq!(schema, "A");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_self_referential_any_of_property() {
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
    A:
      type: object
      properties:
        child:
          anyOf:
            - $ref: '#/components/schemas/A'
"##,
    );
    match err {
        ValidationError::RecursiveAnyOf { context, schema } => {
            assert_eq!(context, "schema `A`");
            assert_eq!(schema, "A");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_recursive_any_of_through_alias() {
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
    A:
      anyOf:
        - $ref: '#/components/schemas/Alias'
    Alias:
      $ref: '#/components/schemas/B'
    B:
      anyOf:
        - $ref: '#/components/schemas/A'
"##,
    );
    match err {
        ValidationError::RecursiveAnyOf { context, schema } => {
            assert_eq!(context, "schema `A`");
            assert_eq!(schema, "A");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_recursive_discriminator_branch() {
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
    C2:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
        u:
          oneOf:
            - $ref: '#/components/schemas/C2'
            - $ref: '#/components/schemas/C3'
          discriminator:
            propertyName: kind
    C3:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
"##,
    );
    match err {
        ValidationError::RecursiveDiscriminatorBranch { context, schema } => {
            assert_eq!(context, "schema `C2`");
            assert_eq!(schema, "C2");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_mutually_recursive_discriminator_branches() {
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
    A:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
        u:
          oneOf:
            - $ref: '#/components/schemas/B'
          discriminator:
            propertyName: kind
    B:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
        u:
          oneOf:
            - $ref: '#/components/schemas/A'
          discriminator:
            propertyName: kind
"##,
    );
    match err {
        ValidationError::RecursiveDiscriminatorBranch { context, schema } => {
            assert!(context == "schema `A`" || context == "schema `B`");
            assert!(schema == "A" || schema == "B");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn accepts_repeated_discriminator_branch_across_unions() {
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
    Leaf:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Holder:
      type: object
      required:
        - first
        - second
      properties:
        first:
          oneOf:
            - $ref: '#/components/schemas/Leaf'
          discriminator:
            propertyName: kind
        second:
          oneOf:
            - $ref: '#/components/schemas/Leaf'
          discriminator:
            propertyName: kind
"##;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let holder = find_struct(&types, "Holder");
    assert_eq!(field_names(holder), ["first", "second"]);
    assert_field(holder, "first", "HolderFirst<S>");
    assert_field(holder, "second", "HolderSecond<S>");
}

#[test]
fn parses_nullable_nested_discriminated_one_of_single_branch() {
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
    Tool:
      type: object
      required:
        - name
      properties:
        name:
          type: string
        cache_control:
          anyOf:
            - discriminator:
                propertyName: type
                mapping:
                  ephemeral: '#/components/schemas/CacheControlEphemeral'
              oneOf:
                - $ref: '#/components/schemas/CacheControlEphemeral'
            - type: 'null'
    CacheControlEphemeral:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          const: ephemeral
"##;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let tool = find_struct(&types, "Tool");
    assert_field(tool, "cache_control", "Option<CacheControlEphemeral>");

    assert!(
        !has_item_named(&types, "ToolCacheControl"),
        "single-reference nested union must not synthesize a wrapper component"
    );

    // NOTE: the nested union's discriminator metadata is asserted through the
    // semantic IR.
    let semantic = normalize_spec(spec);
    let tool_ir = definition(&semantic, "Tool");
    let TypeExpr::Object(tool_object) = &tool_ir.schema.ty else {
        panic!("expected Tool object")
    };
    let cache_control_ir = property(tool_object, "cache_control");
    let TypeExpr::Composition(nullable_union) = &cache_control_ir.value.ty else {
        panic!("expected nullable nested union composition")
    };
    assert_eq!(nullable_union.kind, CompositionKind::AnyOf);
    let TypeExpr::Composition(nested) = &nullable_union.branches[0].ty else {
        panic!("expected nested discriminated oneOf")
    };
    assert_eq!(nested.kind, CompositionKind::OneOf);
    let discriminator = nested
        .discriminator
        .as_ref()
        .expect("nested union keeps its discriminator");
    assert_eq!(discriminator.property_name, "type");
    assert_eq!(
        ref_branch_source_name(&semantic, &nested.branches[0]),
        "CacheControlEphemeral"
    );
}

#[test]
fn parses_nullable_nested_discriminated_one_of_multi_branch() {
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
    Widget:
      type: object
      properties:
        status:
          anyOf:
            - discriminator:
                propertyName: type
              oneOf:
                - $ref: '#/components/schemas/StatusOn'
                - $ref: '#/components/schemas/StatusOff'
            - type: 'null'
    StatusOn:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          const: 'on'
    StatusOff:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          const: 'off'
"##;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let widget = find_struct(&types, "Widget");
    assert_field(widget, "status", "Option<WidgetStatus>");

    let status = find_enum(&types, "WidgetStatus");
    // Branches embed the `type` tag property as singleton const fields, so the
    // generated enum is untagged with the branches in declaration order.
    assert_attr_contains(&status.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(status), ["StatusOn", "StatusOff"]);
    assert_eq!(
        variant_payload_types(status),
        [norm_str("StatusOn"), norm_str("StatusOff")]
    );
    let status_on = find_struct(&types, "StatusOn");
    let status_off = find_struct(&types, "StatusOff");
    assert_field(status_on, "r#type", "StatusOnType");
    assert_field(status_off, "r#type", "StatusOffType");

    // NOTE: the nested union's tag metadata is asserted through the semantic
    // IR; the null sibling branch is retained as a `Null` branch.
    let semantic = normalize_spec(spec);
    let widget_ir = definition(&semantic, "Widget");
    let TypeExpr::Object(widget_object) = &widget_ir.schema.ty else {
        panic!("expected Widget object")
    };
    let status_ir = property(widget_object, "status");
    let TypeExpr::Composition(nullable_union) = &status_ir.value.ty else {
        panic!("expected nullable nested union composition")
    };
    assert_eq!(nullable_union.kind, CompositionKind::AnyOf);
    assert!(matches!(nullable_union.branches[1].ty, TypeExpr::Null));
    let TypeExpr::Composition(nested) = &nullable_union.branches[0].ty else {
        panic!("expected nested discriminated oneOf")
    };
    assert_eq!(nested.kind, CompositionKind::OneOf);
    let discriminator = nested
        .discriminator
        .as_ref()
        .expect("nested union keeps its tag");
    assert_eq!(discriminator.property_name, "type");
    assert_eq!(
        ref_branch_source_name(&semantic, &nested.branches[0]),
        "StatusOn"
    );
    assert_eq!(
        ref_branch_source_name(&semantic, &nested.branches[1]),
        "StatusOff"
    );
}

#[test]
fn parses_nested_union_beside_string_branch() {
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
    Entry:
      oneOf:
        - discriminator:
            propertyName: type
          oneOf:
            - $ref: '#/components/schemas/AgentA'
            - $ref: '#/components/schemas/AgentB'
        - type: string
    AgentA:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          const: a
    AgentB:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          const: b
"##;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let entry = find_enum(&types, "Entry");
    assert_attr_contains(&entry.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(entry), ["Union", "String"]);
    assert_eq!(
        variant_payload_types(entry),
        [norm_str("EntryUnion"), norm_str("S")]
    );

    let nested = find_enum(&types, "EntryUnion");
    assert_attr_contains(&nested.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(nested), ["AgentA", "AgentB"]);
    assert_eq!(
        variant_payload_types(nested),
        [norm_str("AgentA"), norm_str("AgentB")]
    );

    // NOTE: the nested union's tag metadata is asserted through the semantic
    // IR.
    let semantic = normalize_spec(spec);
    let entry_ir = composition(&semantic, "Entry");
    let TypeExpr::Composition(nested_ir) = &entry_ir.branches[0].ty else {
        panic!("expected nested union branch")
    };
    let discriminator = nested_ir
        .discriminator
        .as_ref()
        .expect("nested union keeps its tag");
    assert_eq!(discriminator.property_name, "type");
}

#[test]
fn does_not_collapse_internally_tagged_single_branch_nested_union() {
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
    Holder:
      type: object
      properties:
        item:
          anyOf:
            - discriminator:
                propertyName: type
              oneOf:
                - $ref: '#/components/schemas/Plain'
            - type: 'null'
    Plain:
      type: object
      required:
        - id
      properties:
        id:
          type: string
"##;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let holder = find_struct(&types, "Holder");
    assert_field(holder, "item", "Option<HolderItem<S>>");

    // NOTE: the private model's tag_value fact is now covered by the generated
    // serde rename on the variant.
    let item = find_enum(&types, "HolderItem");
    assert_attr_contains(&item.attrs, "cfg_attr", r#"serde(tag = "type")"#);
    assert_eq!(variant_names(item), ["Plain"]);
    assert_attr_contains(
        &variant(item, "Plain").attrs,
        "cfg_attr",
        r#"serde(rename = "Plain")"#,
    );
}

#[test]
fn rejects_nested_plain_one_of_branch() {
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
    A:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Wrapper:
      type: object
      properties:
        value:
          anyOf:
            - oneOf:
                - $ref: '#/components/schemas/A'
            - type: 'null'
"##,
    );
    match err {
        ValidationError::UnsupportedAnyOfBranch { context, index } => {
            assert_eq!(context, "property `Wrapper.value`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_nested_discriminated_any_of_branch() {
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
    A:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          const: a
    Wrapper:
      type: object
      properties:
        value:
          anyOf:
            - discriminator:
                propertyName: type
              anyOf:
                - $ref: '#/components/schemas/A'
            - type: 'null'
"##,
    );
    match err {
        ValidationError::UnsupportedAnyOfBranch { context, index } => {
            assert_eq!(context, "property `Wrapper.value`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_invalid_mapping_inside_nested_union() {
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
    Tool:
      type: object
      properties:
        cache_control:
          anyOf:
            - discriminator:
                propertyName: type
                mapping:
                  permanent: '#/components/schemas/CacheControlEphemeral'
              oneOf:
                - $ref: '#/components/schemas/CacheControlEphemeral'
            - type: 'null'
    CacheControlEphemeral:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          const: ephemeral
"##,
    );
    match err {
        ValidationError::DiscriminatorMappingValueMismatch {
            context,
            schema,
            value,
            actual,
        } => {
            assert_eq!(context, "property `Tool.cache_control`.anyOf[0]");
            assert_eq!(schema, "CacheControlEphemeral");
            assert_eq!(value, "permanent");
            assert_eq!(actual, "ephemeral");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_recursive_nested_union_reference() {
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
    Wrapper:
      anyOf:
        - discriminator:
            propertyName: type
          oneOf:
            - $ref: '#/components/schemas/BranchA'
            - $ref: '#/components/schemas/BranchB'
        - type: string
    BranchA:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          const: a
        wrapper:
          $ref: '#/components/schemas/Wrapper'
    BranchB:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          const: b
"##,
    );
    match err {
        ValidationError::RecursiveAnyOf { context, schema } => {
            assert!(context == "schema `Wrapper`" || context == "schema `BranchA`");
            assert!(schema == "Wrapper" || schema == "BranchA");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn unwraps_annotation_only_all_of_ref_wrapper_union_branch() {
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
    Params:
      oneOf:
        - title: Auto params
          description: Annotated reference branch.
          allOf:
            - $ref: '#/components/schemas/AutoParams'
          x-stainless-skip:
            - go
            - cli
        - $ref: '#/components/schemas/ManualParams'
    AutoParams:
      type: object
      required:
        - budget
      properties:
        budget:
          type: integer
    ManualParams:
      type: object
      required:
        - level
      properties:
        level:
          type: string
"##;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let params = find_enum(&types, "Params");
    assert_attr_contains(&params.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(params), ["AutoParams", "ManualParams"]);
    assert_eq!(
        variant_payload_types(params),
        [norm_str("AutoParams"), norm_str("ManualParams<S>")]
    );
}

#[test]
fn rejects_all_of_union_branch_with_multiple_refs() {
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
    A:
      type: object
      properties:
        id:
          type: string
    B:
      type: object
      properties:
        name:
          type: string
    Broken:
      oneOf:
        - allOf:
            - $ref: '#/components/schemas/A'
            - $ref: '#/components/schemas/B'
"##,
    );
    match err {
        ValidationError::UnsupportedOneOfBranch { context, index } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_all_of_ref_wrapper_union_branch_with_required_sibling() {
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
    A:
      type: object
      properties:
        id:
          type: string
    Broken:
      anyOf:
        - description: Annotated reference branch.
          allOf:
            - $ref: '#/components/schemas/A'
          required:
            - id
"##,
    );
    match err {
        ValidationError::UnsupportedAnyOfBranch { context, index } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_all_of_ref_wrapper_union_branch_with_satay_extension() {
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
    A:
      type: object
      properties:
        id:
          type: string
    Broken:
      oneOf:
        - allOf:
            - $ref: '#/components/schemas/A'
          x-satay:
            parse-as: u32
"##,
    );
    match err {
        ValidationError::UnsupportedOneOfBranch { context, index } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_all_of_union_branch_with_inline_object_entry() {
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
    Broken:
      oneOf:
        - allOf:
            - type: object
              properties:
                id:
                  type: string
"##,
    );
    match err {
        ValidationError::UnsupportedOneOfBranch { context, index } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_wrapped_ref_union_branch_duplicating_direct_ref_branch() {
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
    A:
      type: object
      properties:
        id:
          type: string
    Broken:
      oneOf:
        - $ref: '#/components/schemas/A'
        - description: Annotated duplicate of the first branch.
          allOf:
            - $ref: '#/components/schemas/A'
"##,
    );
    match err {
        ValidationError::ShadowedUnionBranch {
            context,
            keyword,
            index,
            shadowed_by,
        } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "oneOf");
            assert_eq!(index, 1);
            assert_eq!(shadowed_by, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_recursive_any_of_through_wrapped_ref_branch() {
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
    Wrapper:
      anyOf:
        - title: Annotated target reference.
          allOf:
            - $ref: '#/components/schemas/Target'
        - type: string
    Target:
      anyOf:
        - $ref: '#/components/schemas/Wrapper'
        - type: integer
"##,
    );
    match err {
        ValidationError::RecursiveAnyOf { context, schema } => {
            assert!(context == "schema `Wrapper`" || context == "schema `Target`");
            assert!(schema == "Wrapper" || schema == "Target");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn ignores_discriminator_on_plain_object_schema() {
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
    Message:
      type: object
      required:
        - role
      properties:
        role:
          type: string
      discriminator:
        propertyName: role
"##;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let message = find_struct(&types, "Message");
    assert_eq!(field_names(message), ["role"]);
    assert_field(message, "role", "S");
    // The field stays required: no optional-default serde attrs were rendered.
    assert!(!contains_tokens(
        message,
        "serde(default, skip_serializing_if = \"Option::is_none\")"
    ));
}

#[test]
fn parses_discriminator_union_with_object_type_sibling() {
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
    Dog:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          enum:
            - dog
    Cat:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          enum:
            - cat
    Pet:
      type: object
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
"##;

    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let pet = find_enum(&types, "Pet");
    assert_attr_contains(&pet.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(pet), ["Dog", "Cat"]);
    assert_eq!(
        variant_payload_types(pet),
        [norm_str("Dog"), norm_str("Cat")]
    );

    // Branches embed the `kind` tag property as singleton fields.
    let dog = find_struct(&types, "Dog");
    let cat = find_struct(&types, "Cat");
    assert_field(dog, "kind", "DogKind");
    assert_field(cat, "kind", "CatKind");

    // NOTE: the tag metadata is asserted through the semantic IR.
    let semantic = normalize_spec(spec);
    let pet_ir = composition(&semantic, "Pet");
    let discriminator = pet_ir
        .discriminator
        .as_ref()
        .expect("embedded discriminator tag");
    assert_eq!(discriminator.property_name, "kind");
}

#[test]
fn rejects_discriminator_union_with_non_object_type_sibling() {
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
    Dog:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          enum:
            - dog
    Cat:
      type: object
      required:
        - kind
      properties:
        kind:
          type: string
          enum:
            - cat
    Pet:
      type: string
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
"##,
    );

    match err {
        ValidationError::UnsupportedAnyOfSiblingKeyword { context, keyword } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(keyword, "type");
        }
        other => panic!("unexpected error: {other}"),
    }
}
