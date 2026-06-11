use super::*;

#[test]
fn parses_any_of_component_and_inline_refs_into_ir() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let search_result = component(&api, "SearchResult");
    match &search_result.kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            let variants = &union.variants;
            assert_eq!(variants.len(), 2);
            assert_eq!(variants[0].rust_name, "User");
            assert_eq!(variants[0].ty, TypeRef::Named("User".to_owned()));
            assert_eq!(variants[1].rust_name, "Organization");
            assert_eq!(variants[1].ty, TypeRef::Named("Organization".to_owned()));
        }
        other => panic!("expected SearchResult union, got {other:?}"),
    }

    let envelope = component(&api, "Envelope");
    match &envelope.kind {
        ComponentKind::Struct(fields) => {
            let item = field(fields, "item");
            assert_eq!(item.ty, TypeRef::Named("EnvelopeItem".to_owned()));
            assert!(item.required);
        }
        other => panic!("expected Envelope struct, got {other:?}"),
    }

    let envelope_item = component(&api, "EnvelopeItem");
    match &envelope_item.kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            let variants = &union.variants;
            assert_eq!(variants[0].rust_name, "Organization");
            assert_eq!(variants[0].ty, TypeRef::Named("Organization".to_owned()));
            assert_eq!(variants[1].rust_name, "User");
            assert_eq!(variants[1].ty, TypeRef::Named("User".to_owned()));
        }
        other => panic!("expected EnvelopeItem union, got {other:?}"),
    }

    assert_eq!(
        api.operations[0].responses[0].body,
        Some(TypeRef::Named("SearchResult".to_owned()))
    );
}

#[test]
fn parses_one_of_component_and_inline_refs_into_ir() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let assistant_tool = component(&api, "AssistantTool");
    match &assistant_tool.kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            let variants = &union.variants;
            assert_eq!(variants.len(), 3);
            assert_eq!(variants[0].rust_name, "AssistantToolsCode");
            assert_eq!(
                variants[0].ty,
                TypeRef::Named("AssistantToolsCode".to_owned())
            );
            assert_eq!(variants[1].rust_name, "AssistantToolsFileSearch");
            assert_eq!(
                variants[1].ty,
                TypeRef::Named("AssistantToolsFileSearch".to_owned())
            );
            assert_eq!(variants[2].rust_name, "AssistantToolsFunction");
            assert_eq!(
                variants[2].ty,
                TypeRef::Named("AssistantToolsFunction".to_owned())
            );
        }
        other => panic!("expected AssistantTool union, got {other:?}"),
    }

    let assistant = component(&api, "AssistantObject");
    match &assistant.kind {
        ComponentKind::Struct(fields) => {
            let tools = field(fields, "tools");
            assert_eq!(
                tools.ty,
                TypeRef::Array(Box::new(TypeRef::Named(
                    "AssistantObjectToolsItem".to_owned()
                )))
            );
            assert!(tools.required);
        }
        other => panic!("expected AssistantObject struct, got {other:?}"),
    }

    let tools_item = component(&api, "AssistantObjectToolsItem");
    match &tools_item.kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            let variants = &union.variants;
            assert_eq!(variants[0].rust_name, "AssistantToolsCode");
            assert_eq!(variants[1].rust_name, "AssistantToolsFileSearch");
            assert_eq!(variants[2].rust_name, "AssistantToolsFunction");
        }
        other => panic!("expected AssistantObjectToolsItem union, got {other:?}"),
    }
}

#[test]
fn parses_one_of_with_inline_singleton_string_enum_branch() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let format = component(&api, "AssistantsApiResponseFormatOption");
    match &format.kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            assert_eq!(union.variants.len(), 4);
            assert_eq!(union.variants[0].rust_name, "Auto");
            assert_eq!(
                union.variants[0].ty,
                TypeRef::Named("AssistantsApiResponseFormatOptionAuto".to_owned())
            );
            assert_eq!(union.variants[1].rust_name, "ResponseFormatText");
            assert_eq!(
                union.variants[1].ty,
                TypeRef::Named("ResponseFormatText".to_owned())
            );
        }
        other => panic!("expected AssistantsApiResponseFormatOption union, got {other:?}"),
    }

    let auto = component(&api, "AssistantsApiResponseFormatOptionAuto");
    match &auto.kind {
        ComponentKind::Enum(enum_) => {
            assert_eq!(enum_.variants.len(), 1);
            assert_eq!(enum_.variants[0].wire_name, "auto");
            assert_eq!(enum_.variants[0].rust_name, "Auto");
            assert_eq!(enum_.fallback, EnumFallback::None);
        }
        other => panic!("expected AssistantsApiResponseFormatOptionAuto enum, got {other:?}"),
    }
}

#[test]
fn parses_one_of_with_inline_multi_value_string_enum_branch() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let option = component(&api, "AssistantsApiToolChoiceOption");
    match &option.kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            assert_eq!(union.variants.len(), 2);
            assert_eq!(union.variants[0].rust_name, "Enum");
            assert_eq!(
                union.variants[0].ty,
                TypeRef::Named("AssistantsApiToolChoiceOptionEnum".to_owned())
            );
            assert_eq!(union.variants[1].rust_name, "AssistantsNamedToolChoice");
            assert_eq!(
                union.variants[1].ty,
                TypeRef::Named("AssistantsNamedToolChoice".to_owned())
            );
        }
        other => panic!("expected AssistantsApiToolChoiceOption union, got {other:?}"),
    }

    let enum_branch = component(&api, "AssistantsApiToolChoiceOptionEnum");
    match &enum_branch.kind {
        ComponentKind::Enum(enum_) => {
            assert_eq!(enum_.variants.len(), 3);
            assert_eq!(enum_.variants[0].wire_name, "none");
            assert_eq!(enum_.variants[0].rust_name, "None");
            assert_eq!(enum_.variants[1].wire_name, "auto");
            assert_eq!(enum_.variants[1].rust_name, "Auto");
            assert_eq!(enum_.variants[2].wire_name, "required");
            assert_eq!(enum_.variants[2].rust_name, "Required");
            assert_eq!(enum_.fallback, EnumFallback::None);
        }
        other => panic!("expected AssistantsApiToolChoiceOptionEnum enum, got {other:?}"),
    }
}

#[test]
fn parses_one_of_with_nullable_inline_primitive_branches() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let message = component(&api, "Message");
    match &message.kind {
        ComponentKind::Struct(fields) => {
            let content = field(fields, "content");
            assert_eq!(
                content.ty,
                TypeRef::Option(Box::new(TypeRef::Named("MessageContent".to_owned())))
            );
            assert!(!content.required);
        }
        other => panic!("expected Message struct, got {other:?}"),
    }

    let content = component(&api, "MessageContent");
    match &content.kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            assert_eq!(union.variants.len(), 2);
            assert_eq!(union.variants[0].rust_name, "String");
            assert_eq!(union.variants[0].ty, TypeRef::String);
            assert_eq!(union.variants[1].rust_name, "Array");
            assert_eq!(
                union.variants[1].ty,
                TypeRef::Array(Box::new(TypeRef::Named("ContentPart".to_owned())))
            );
        }
        other => panic!("expected MessageContent union, got {other:?}"),
    }
}

#[test]
fn parses_any_of_with_inline_primitive_branches() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let value = component(&api, "PrimitiveValue");
    match &value.kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            assert_eq!(union.variants.len(), 5);
            assert_eq!(union.variants[0].rust_name, "String");
            assert_eq!(union.variants[0].ty, TypeRef::String);
            assert_eq!(union.variants[1].rust_name, "Integer");
            assert_eq!(union.variants[1].ty, TypeRef::Integer(IntegerType::I64));
            assert_eq!(union.variants[2].rust_name, "Number");
            assert_eq!(union.variants[2].ty, TypeRef::F64);
            assert_eq!(union.variants[3].rust_name, "Boolean");
            assert_eq!(union.variants[3].ty, TypeRef::Bool);
            assert_eq!(union.variants[4].rust_name, "Array");
            assert_eq!(
                union.variants[4].ty,
                TypeRef::Array(Box::new(TypeRef::String))
            );
        }
        other => panic!("expected PrimitiveValue union, got {other:?}"),
    }
}

#[test]
fn parses_any_of_open_string_enum_branch() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let transcription = component(&api, "AudioTranscription");
    match &transcription.kind {
        ComponentKind::Struct(fields) => {
            let model = field(fields, "model");
            assert_eq!(
                model.ty,
                TypeRef::Named("AudioTranscriptionModel".to_owned())
            );
            assert!(!model.required);
        }
        other => panic!("expected AudioTranscription struct, got {other:?}"),
    }

    let model = component(&api, "AudioTranscriptionModel");
    match &model.kind {
        ComponentKind::Enum(enum_) => {
            let variants = &enum_.variants;
            assert_eq!(variants.len(), 3);
            assert_eq!(variants[0].wire_name, "whisper-1");
            assert_eq!(variants[0].rust_name, "Whisper1");
            assert_eq!(variants[1].wire_name, "gpt-4o-mini-transcribe");
            assert_eq!(variants[1].rust_name, "Gpt4oMiniTranscribe");
            assert_eq!(variants[2].wire_name, "gpt-4o-transcribe");
            assert_eq!(variants[2].rust_name, "Gpt4oTranscribe");
            assert_eq!(enum_.fallback, EnumFallback::OtherString);
        }
        other => panic!("expected AudioTranscriptionModel enum, got {other:?}"),
    }
}

#[test]
fn parses_any_of_open_string_enum_with_annotation_only_string_branch() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let model = component(&api, "Model");
    assert_eq!(
        model.description.as_deref(),
        Some("Known model identifiers.")
    );
    match &model.kind {
        ComponentKind::Enum(enum_) => {
            assert_eq!(enum_.variants.len(), 1);
            assert_eq!(enum_.variants[0].wire_name, "known-model");
            assert_eq!(enum_.fallback, EnumFallback::OtherString);
        }
        other => panic!("expected Model enum, got {other:?}"),
    }
}

#[test]
fn parses_any_of_open_string_enum_prefers_outer_description() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let model = component(&api, "Model");
    assert_eq!(
        model.description.as_deref(),
        Some("Preferred model identifier.")
    );
    assert!(
        matches!(&model.kind, ComponentKind::Enum(enum_) if enum_.fallback == EnumFallback::OtherString)
    );
}

#[test]
fn parses_constrained_string_branch_as_plain_union_not_open_enum() {
    let api = parse_valid(
        r#"
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
          minLength: 1
        - type: string
          enum:
            - known-model
"#,
    );

    let model = component(&api, "Model");
    match &model.kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            let variants = &union.variants;
            assert_eq!(variants.len(), 2);
            assert_eq!(variants[0].rust_name, "String");
            match &variants[0].ty {
                TypeRef::Constrained { rust_name, inner } => {
                    assert_eq!(rust_name, "ModelString");
                    assert_eq!(inner.as_ref(), &TypeRef::String);
                }
                other => panic!("expected constrained string variant, got {other:?}"),
            }
            assert_eq!(variants[1].rust_name, "KnownModel");
            assert_eq!(variants[1].ty, TypeRef::Named("ModelKnownModel".to_owned()));
        }
        other => panic!("expected Model union, got {other:?}"),
    }

    let known_model = component(&api, "ModelKnownModel");
    match &known_model.kind {
        ComponentKind::Enum(enum_) => {
            assert_eq!(enum_.variants.len(), 1);
            assert_eq!(enum_.variants[0].wire_name, "known-model");
            assert_eq!(enum_.fallback, EnumFallback::None);
        }
        other => panic!("expected ModelKnownModel enum, got {other:?}"),
    }
}

#[test]
fn parses_union_schemas_with_vendor_metadata_extensions() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let assistant_stream_event = component(&api, "AssistantStreamEvent");
    match &assistant_stream_event.kind {
        ComponentKind::Union(union) => {
            assert!(union.tag.is_none());
            assert_eq!(union.variants.len(), 2);
        }
        other => panic!("expected AssistantStreamEvent union, got {other:?}"),
    }

    let search_result = component(&api, "SearchResult");
    assert!(matches!(&search_result.kind, ComponentKind::Union(union) if union.tag.is_none()));

    let tagged_event = component(&api, "TaggedEvent");
    match &tagged_event.kind {
        ComponentKind::Union(union) => {
            assert_eq!(
                union.tag.as_ref().map(|tag| tag.property_name.as_str()),
                Some("event")
            );
        }
        other => panic!("expected TaggedEvent union, got {other:?}"),
    }
}

#[test]
fn parses_discriminator_with_embedded_singleton_type_fields_into_ir() {
    let api = parse_valid(
        r#"
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
"#,
    );

    let function_tool_call = component(&api, "FunctionToolCall");
    match &function_tool_call.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(field(fields, "type").rust_name, "r#type");
        }
        other => panic!("expected FunctionToolCall struct, got {other:?}"),
    }

    let tool_call = component(&api, "ToolCall");
    match &tool_call.kind {
        ComponentKind::Union(union) => {
            let tag = union.tag.as_ref().expect("embedded discriminator tag");
            assert_eq!(tag.property_name, "type");
            assert_eq!(tag.style, UnionTagStyle::EmbeddedField);
            assert_eq!(union.variants.len(), 2);
            assert!(
                union
                    .variants
                    .iter()
                    .all(|variant| variant.tag_value.is_none())
            );
        }
        other => panic!("expected ToolCall union, got {other:?}"),
    }
}

#[test]
fn rejects_empty_any_of() {
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
      anyOf: []
"##,
    );
    match err {
        ValidationError::EmptyAnyOf { context } => {
            assert_eq!(context, "schema `Broken`");
        }
        other => panic!("unexpected error: {other}"),
    }
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
            assert_eq!(expected, "a required non-null singleton string enum");
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
