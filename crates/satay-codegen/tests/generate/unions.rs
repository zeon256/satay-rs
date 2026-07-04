use std::fs;

use crate::ast::*;
use crate::common::*;

const NULLABLE_INLINE_PRIMITIVE_ONE_OF: &str = r##"
openapi: 3.1.0
info:
  title: Message API
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
"##;

#[test]
fn any_of_generates_untagged_union_types() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Search API
  version: 1.0.0
paths:
  /search:
    get:
      operationId: search
      responses:
        '200':
          description: Search result
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
"##,
    )
    .expect("generate anyOf fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "SearchResult");
    assert_doc(&union.attrs, "A search result.");
    assert_attr_contains(&union.attrs, "cfg_attr", "serde(untagged)");

    assert_eq!(variant_names(union), ["User", "Organization"]);
    assert_eq!(norm(&variant(union, "User").fields), norm_str("(User)"));
    assert_eq!(
        norm(&variant(union, "Organization").fields),
        norm_str("(Organization)")
    );
}

#[test]
fn one_of_generates_untagged_union_types() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Assistant API
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
      description: An assistant tool.
      oneOf:
        - $ref: '#/components/schemas/AssistantToolsCode'
        - $ref: '#/components/schemas/AssistantToolsFileSearch'
        - $ref: '#/components/schemas/AssistantToolsFunction'
      x-oaiMeta:
        name: Assistant tools
        beta: true
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
"##,
    )
    .expect("generate oneOf fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "AssistantTool");
    assert_doc(&union.attrs, "An assistant tool.");
    assert_attr_contains(&union.attrs, "cfg_attr", "serde(untagged)");

    assert_eq!(
        variant_names(union),
        [
            "AssistantToolsCode",
            "AssistantToolsFileSearch",
            "AssistantToolsFunction"
        ]
    );
    assert_eq!(
        norm(&variant(union, "AssistantToolsCode").fields),
        norm_str("(AssistantToolsCode)")
    );
    assert_eq!(
        norm(&variant(union, "AssistantToolsFileSearch").fields),
        norm_str("(AssistantToolsFileSearch)")
    );
    assert_eq!(
        norm(&variant(union, "AssistantToolsFunction").fields),
        norm_str("(AssistantToolsFunction)")
    );

    let assistant = find_struct(&types_rs, "AssistantObject");
    assert_field(assistant, "tools", "Vec<AssistantObjectToolsItem>");
    let code_type = find_enum(&types_rs, "AssistantToolsCodeType");
    assert_eq!(variant_names(code_type), ["CodeInterpreter"]);
    let tools_item = find_enum(&types_rs, "AssistantObjectToolsItem");
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
fn one_of_generates_inline_singleton_string_enum_branch() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Response Format API
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
"##,
    )
    .expect("generate oneOf inline singleton enum fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "AssistantsApiResponseFormatOption");
    assert_doc(&union.attrs, "Response format option.");
    assert_attr_contains(&union.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(
        variant_names(union),
        [
            "Auto",
            "ResponseFormatText",
            "ResponseFormatJsonObject",
            "ResponseFormatJsonSchema"
        ]
    );
    assert_eq!(
        norm(&variant(union, "Auto").fields),
        norm_str("(AssistantsApiResponseFormatOptionAuto)")
    );

    let auto = find_enum(&types_rs, "AssistantsApiResponseFormatOptionAuto");
    assert_eq!(variant_names(auto), ["Auto"]);
    assert!(!contains_tokens(
        &types_rs,
        "AssistantsApiResponseFormatOptionAuto :: Unknown"
    ));
}

#[test]
fn one_of_generates_inline_multi_value_string_enum_branch() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Tool Choice API
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
"##,
    )
    .expect("generate oneOf inline multi-value enum fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "AssistantsApiToolChoiceOption");
    assert_doc(&union.attrs, "Tool choice option.");
    assert_attr_contains(&union.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(union), ["Enum", "AssistantsNamedToolChoice"]);
    assert_eq!(
        norm(&variant(union, "Enum").fields),
        norm_str("(AssistantsApiToolChoiceOptionEnum)")
    );

    let enum_branch = find_enum(&types_rs, "AssistantsApiToolChoiceOptionEnum");
    assert_eq!(variant_names(enum_branch), ["None", "Auto", "Required"]);
    assert!(!contains_tokens(
        &types_rs,
        "AssistantsApiToolChoiceOptionEnum :: Other"
    ));
}

#[test]
fn one_of_generates_nullable_inline_primitive_union_branch() {
    let files = satay_codegen::generate(NULLABLE_INLINE_PRIMITIVE_ONE_OF)
        .expect("generate nullable oneOf inline primitive fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let message = find_struct(&types_rs, "Message");
    assert_field(message, "content", "Option<MessageContent>");

    let content = find_enum(&types_rs, "MessageContent");
    assert_attr_contains(&content.attrs, "cfg_attr", "serde(untagged)");
    assert_eq!(variant_names(content), ["String", "Array"]);
    assert_eq!(
        norm(&variant(content, "String").fields),
        norm_str("(String)")
    );
    assert_eq!(
        norm(&variant(content, "Array").fields),
        norm_str("(Vec<ContentPart>)")
    );
}

#[test]
fn any_of_discriminator_generates_tagged_union_types() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Search API
  version: 1.0.0
paths:
  /search:
    get:
      operationId: search
      responses:
        '200':
          description: Search result
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
      anyOf:
        - $ref: '#/components/schemas/User'
        - $ref: '#/components/schemas/Organization'
      discriminator:
        propertyName: kind
"##,
    )
    .expect("generate discriminator anyOf fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "SearchResult");
    assert_attr_contains(&union.attrs, "cfg_attr", r#"serde(tag = "kind")"#);
    assert!(!contains_tokens(&types_rs, "serde(untagged)"));
    assert_attr_contains(
        &variant(union, "User").attrs,
        "cfg_attr",
        r#"serde(rename = "User")"#,
    );
    assert_attr_contains(
        &variant(union, "Organization").attrs,
        "cfg_attr",
        r#"serde(rename = "Organization")"#,
    );
}

#[test]
fn one_of_discriminator_mapping_generates_variant_renames() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Pet API
  version: 1.0.0
paths:
  /pet:
    get:
      operationId: getPet
      responses:
        '200':
          description: Pet
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Pet'
components:
  schemas:
    Dog:
      type: object
      required:
        - name
      properties:
        name:
          type: string
    Cat:
      type: object
      required:
        - name
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
          dog: '#/components/schemas/Dog'
"##,
    )
    .expect("generate discriminator oneOf fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "Pet");
    assert_attr_contains(&union.attrs, "cfg_attr", r#"serde(tag = "kind")"#);
    assert_attr_contains(
        &variant(union, "Dog").attrs,
        "cfg_attr",
        r#"serde(rename = "dog")"#,
    );
    assert_attr_contains(
        &variant(union, "Cat").attrs,
        "cfg_attr",
        r#"serde(rename = "Cat")"#,
    );
}

#[test]
fn discriminator_with_embedded_type_field_generates_untagged_union() {
    let files = satay_codegen::generate(
        r##"
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
"##,
    )
    .expect("generate embedded discriminator fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "ToolCall");
    assert_attr_contains(&union.attrs, "cfg_attr", "serde(untagged)");
    assert!(!contains_tokens(&types_rs, r#"serde(tag = "type")"#));

    let function_tool_call = find_struct(&types_rs, "FunctionToolCall");
    assert_field(function_tool_call, "r#type", "FunctionToolCallType");
    assert!(!contains_tokens(
        field(function_tool_call, "r#type"),
        r#"serde(rename = "type")"#
    ));
}

#[test]
fn discriminator_with_const_type_field_generates_untagged_union() {
    let files = satay_codegen::generate(
        r##"
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
"##,
    )
    .expect("generate const embedded discriminator fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let union = find_enum(&types_rs, "ToolCall");
    assert_attr_contains(&union.attrs, "cfg_attr", "serde(untagged)");
    assert!(!contains_tokens(&types_rs, r#"serde(tag = "type")"#));

    let function_tool_call = find_struct(&types_rs, "FunctionToolCall");
    assert_field(function_tool_call, "r#type", "FunctionToolCallType");
    assert!(!contains_tokens(
        field(function_tool_call, "r#type"),
        r#"serde(rename = "type")"#
    ));
}

#[test]
fn generated_any_of_deserializes_with_first_matching_branch() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Entity API
  version: 1.0.0
paths:
  /entity:
    get:
      operationId: getEntity
      responses:
        '200':
          description: Entity
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Entity'
components:
  schemas:
    Loose:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Specific:
      type: object
      required:
        - id
        - slug
      properties:
        id:
          type: string
        slug:
          type: string
    Entity:
      anyOf:
        - $ref: '#/components/schemas/Loose'
        - $ref: '#/components/schemas/Specific'
"##,
    )
    .expect("generate anyOf runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn any_of_uses_first_matching_branch() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"1","slug":"specific"}"#.to_vec(),
        };

        let decoded = decode_get_entity_response(response).expect("decoded response");
        match decoded {
            GetEntityResponse::Ok(Entity::Loose(value)) => {
                assert_eq!(value.id, "1");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "anyOf generated crate tests");
}

#[test]
fn generated_one_of_tool_union_deserializes_by_singleton_type_field() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Assistant API
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
        - function
      properties:
        type:
          type: string
          enum:
            - function
        function:
          type: string
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
"##,
    )
    .expect("generate oneOf runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn one_of_tool_union_deserializes_by_type_field() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"tools":[{"type":"function","function":"lookup"}]}"#.to_vec(),
        };

        let decoded = decode_get_assistant_response(response).expect("decoded response");
        match decoded {
            GetAssistantResponse::Ok(value) => match &value.tools[0] {
                AssistantObjectToolsItem::AssistantToolsFunction(tool) => {
                    assert_eq!(tool.function, "lookup");
                }
                other => panic!("unexpected tool: {other:?}"),
            },
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "oneOf generated crate tests");
}

#[test]
fn generated_nullable_inline_primitive_one_of_deserializes_and_serializes() {
    let files = satay_codegen::generate(NULLABLE_INLINE_PRIMITIVE_ONE_OF)
        .expect("generate nullable oneOf inline primitive runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn string_content_deserializes_to_string_variant() {
        let value: Message = serde_json::from_str(r#"{"content":"hello"}"#)
            .expect("message with string content");

        match value.content {
            Some(MessageContent::String(text)) => assert_eq!(text, "hello"),
            other => panic!("unexpected content: {other:?}"),
        }
    }

    #[test]
    fn array_content_deserializes_to_array_variant() {
        let value: Message = serde_json::from_str(
            r#"{"content":[{"type":"text","text":"hello"}]}"#,
        )
        .expect("message with array content");

        match value.content {
            Some(MessageContent::Array(parts)) => {
                assert_eq!(parts.len(), 1);
                assert_eq!(parts[0].text, "hello");
            }
            other => panic!("unexpected content: {other:?}"),
        }
    }

    #[test]
    fn null_content_deserializes_to_none() {
        let value: Message = serde_json::from_str(r#"{"content":null}"#)
            .expect("message with null content");

        assert_eq!(value.content, None);
    }

    #[test]
    fn absent_optional_content_serializes_as_absent() {
        let value = Message { content: None };
        let encoded = serde_json::to_value(value).expect("serialized message");

        assert_eq!(encoded, serde_json::json!({}));
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "nullable oneOf inline primitive generated crate tests",
    );
}

#[test]
fn generated_one_of_inline_singleton_branch_serializes_and_deserializes() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Response Format API
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
      oneOf:
        - type: string
          enum:
            - auto
          x-stainless-const: true
        - $ref: '#/components/schemas/ResponseFormatText'
        - $ref: '#/components/schemas/ResponseFormatJsonObject'
        - $ref: '#/components/schemas/ResponseFormatJsonSchema'
"##,
    )
    .expect("generate oneOf inline singleton runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn one_of_inline_singleton_deserializes_string_branch() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#""auto""#.to_vec(),
        };

        let decoded = decode_get_format_response(response).expect("decoded response");
        match decoded {
            GetFormatResponse::Ok(AssistantsApiResponseFormatOption::Auto(value)) => {
                assert_eq!(value, AssistantsApiResponseFormatOptionAuto::Auto);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn one_of_inline_singleton_deserializes_object_branch() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"type":"json_object"}"#.to_vec(),
        };

        let decoded = decode_get_format_response(response).expect("decoded response");
        match decoded {
            GetFormatResponse::Ok(AssistantsApiResponseFormatOption::ResponseFormatJsonObject(value)) => {
                assert_eq!(value.r#type, ResponseFormatJsonObjectType::JsonObject);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn one_of_inline_singleton_serializes_string_branch() {
        let value = AssistantsApiResponseFormatOption::Auto(
            AssistantsApiResponseFormatOptionAuto::Auto,
        );
        let encoded = serde_json::to_value(value).expect("serialized response format");
        assert_eq!(encoded, serde_json::json!("auto"));
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "oneOf inline singleton generated crate tests",
    );
}

#[test]
fn generated_one_of_inline_multi_value_branch_serializes_and_deserializes() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Tool Choice API
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
    FunctionToolChoice:
      type: object
      required:
        - name
      properties:
        name:
          type: string
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
        function:
          $ref: '#/components/schemas/FunctionToolChoice'
    AssistantsApiToolChoiceOption:
      oneOf:
        - type: string
          enum:
            - none
            - auto
            - required
        - $ref: '#/components/schemas/AssistantsNamedToolChoice'
"##,
    )
    .expect("generate oneOf inline multi-value runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn one_of_inline_multi_value_deserializes_string_branch() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#""auto""#.to_vec(),
        };

        let decoded = decode_get_tool_choice_response(response).expect("decoded response");
        match decoded {
            GetToolChoiceResponse::Ok(AssistantsApiToolChoiceOption::Enum(value)) => {
                assert_eq!(value, AssistantsApiToolChoiceOptionEnum::Auto);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn one_of_inline_multi_value_deserializes_object_branch() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"type":"function","function":{"name":"my_function"}}"#.to_vec(),
        };

        let decoded = decode_get_tool_choice_response(response).expect("decoded response");
        match decoded {
            GetToolChoiceResponse::Ok(
                AssistantsApiToolChoiceOption::AssistantsNamedToolChoice(value),
            ) => {
                assert_eq!(value.r#type, AssistantsNamedToolChoiceType::Function);
                assert_eq!(
                    value.function.expect("function choice").name,
                    "my_function"
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn one_of_inline_multi_value_serializes_string_branch() {
        let value = AssistantsApiToolChoiceOption::Enum(
            AssistantsApiToolChoiceOptionEnum::Required,
        );
        let encoded = serde_json::to_value(value).expect("serialized tool choice");
        assert_eq!(encoded, serde_json::json!("required"));
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "oneOf inline multi-value generated crate tests",
    );
}

#[test]
fn generated_discriminator_union_serializes_and_deserializes_with_tag() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Pet API
  version: 1.0.0
paths:
  /pet:
    get:
      operationId: getPet
      responses:
        '200':
          description: Pet
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Pet'
components:
  schemas:
    Dog:
      type: object
      required:
        - name
        - barkVolume
      properties:
        name:
          type: string
        barkVolume:
          type: integer
    Cat:
      type: object
      required:
        - name
        - lives
      properties:
        name:
          type: string
        lives:
          type: integer
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Dog'
        - $ref: '#/components/schemas/Cat'
      discriminator:
        propertyName: kind
        mapping:
          dog: '#/components/schemas/Dog'
          cat: Cat
"##,
    )
    .expect("generate discriminator runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn tagged_union_deserializes_response() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"kind":"cat","name":"Milo","lives":9}"#.to_vec(),
        };

        let decoded = decode_get_pet_response(response).expect("decoded response");
        match decoded {
            GetPetResponse::Ok(Pet::Cat(value)) => {
                assert_eq!(value.name, "Milo");
                assert_eq!(value.lives, 9);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn tagged_union_serializes_tag() {
        let value = Pet::Dog(Dog {
            name: "Rex".to_owned(),
            bark_volume: 7,
        });
        let encoded = serde_json::to_value(value).expect("serialized pet");
        assert_eq!(
            encoded,
            serde_json::json!({
                "kind": "dog",
                "name": "Rex",
                "barkVolume": 7
            })
        );
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "discriminator generated crate tests",
    );
}

#[test]
fn generated_embedded_discriminator_union_uses_branch_type_field() {
    let files = satay_codegen::generate(
        r##"
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
"##,
    )
    .expect("generate embedded discriminator runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn embedded_tag_union_deserializes_response() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"call_1","type":"custom","custom":"payload"}"#.to_vec(),
        };

        let decoded = decode_get_tool_response(response).expect("decoded response");
        match decoded {
            GetToolResponse::Ok(ToolCall::CustomToolCall(value)) => {
                assert_eq!(value.id, "call_1");
                assert_eq!(value.r#type, CustomToolCallType::Custom);
                assert_eq!(value.custom, "payload");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn embedded_tag_union_serializes_branch_type() {
        let value = ToolCall::FunctionToolCall(FunctionToolCall {
            id: "call_2".to_owned(),
            r#type: FunctionToolCallType::Function,
            function: "lookup".to_owned(),
        });
        let encoded = serde_json::to_value(value).expect("serialized tool call");
        assert_eq!(
            encoded,
            serde_json::json!({
                "id": "call_2",
                "type": "function",
                "function": "lookup"
            })
        );
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "embedded discriminator generated crate tests",
    );
}

#[test]
fn generated_const_embedded_discriminator_union_round_trips() {
    let files = satay_codegen::generate(
        r##"
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
"##,
    )
    .expect("generate const embedded discriminator runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn const_tag_union_deserializes_response() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"call_1","type":"custom","custom":"payload"}"#.to_vec(),
        };

        let decoded = decode_get_tool_response(response).expect("decoded response");
        match decoded {
            GetToolResponse::Ok(ToolCall::CustomToolCall(value)) => {
                assert_eq!(value.id, "call_1");
                assert_eq!(value.r#type, CustomToolCallType::Custom);
                assert_eq!(value.custom, "payload");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn const_tag_union_serializes_branch_type() {
        let value = ToolCall::FunctionToolCall(FunctionToolCall {
            id: "call_2".to_owned(),
            r#type: FunctionToolCallType::Function,
            function: "lookup".to_owned(),
        });
        let encoded = serde_json::to_value(value).expect("serialized tool call");
        assert_eq!(
            encoded,
            serde_json::json!({
                "id": "call_2",
                "type": "function",
                "function": "lookup"
            })
        );
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "const embedded discriminator generated crate tests",
    );
}
