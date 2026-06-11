use std::fs;

use satay_codegen::{Error, ValidationError};

use crate::ast::*;
use crate::common::*;

#[test]
fn all_of_flattens_ref_and_inline_object_branches() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Child API
  version: 1.0.0
paths:
  /child:
    get:
      operationId: getChild
      responses:
        '200':
          description: Child
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Child'
components:
  schemas:
    Base:
      type: object
      required:
        - id
      properties:
        id:
          type: string
          description: Base identifier.
    Decorated:
      allOf:
        - $ref: '#/components/schemas/Base'
        - type: object
          required:
            - tag
          properties:
            tag:
              type: string
    Child:
      description: A flattened child.
      allOf:
        - $ref: '#/components/schemas/Decorated'
        - type: object
          required:
            - name
          properties:
            name:
              type: string
            nickname:
              type: string
"##,
    )
    .expect("generate allOf fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let child = find_struct(&types_rs, "Child");
    assert_doc(&child.attrs, "A flattened child.");
    assert_eq!(field_names(child), ["id", "tag", "name", "nickname"]);
    assert_field(child, "id", "String");
    assert_field(child, "tag", "String");
    assert_field(child, "name", "String");
    assert_field(child, "nickname", "Option<String>");
}

#[test]
fn generated_all_of_struct_decodes_flattened_fields() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Child API
  version: 1.0.0
paths:
  /child:
    get:
      operationId: getChild
      responses:
        '200':
          description: Child
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Child'
components:
  schemas:
    Base:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Child:
      allOf:
        - $ref: '#/components/schemas/Base'
        - type: object
          required:
            - name
          properties:
            name:
              type: string
            nickname:
              type: string
"##,
    )
    .expect("generate allOf runtime fixture");

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
    fn decodes_flattened_all_of_fields() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"base-1","name":"Ada","nickname":"ace"}"#.to_vec(),
        };

        let decoded = decode_get_child_response(response).expect("decoded response");
        match decoded {
            GetChildResponse::Ok(child) => {
                assert_eq!(child.id, "base-1");
                assert_eq!(child.name, "Ada");
                assert_eq!(child.nickname, Some("ace".to_owned()));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "allOf generated crate tests");
}

#[test]
fn inline_all_of_array_items_generate_named_flattened_structs() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Messages API
  version: 1.0.0
paths:
  /messages:
    get:
      operationId: listMessages
      responses:
        '200':
          description: Messages
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ChatCompletionMessageList'
components:
  schemas:
    ChatCompletionRequestMessageContentPartText:
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
    ChatCompletionRequestMessageContentPartImage:
      type: object
      required:
        - type
        - image_url
      properties:
        type:
          type: string
          enum:
            - image_url
        image_url:
          type: string
    ChatCompletionResponseMessage:
      type: object
      required:
        - role
        - content
      properties:
        role:
          type: string
        content:
          type: string
    ChatCompletionMessageList:
      type: object
      required:
        - object
        - data
        - first_id
        - last_id
        - has_more
      properties:
        object:
          type: string
          enum:
            - list
        data:
          type: array
          items:
            allOf:
              - $ref: '#/components/schemas/ChatCompletionResponseMessage'
              - type: object
                required:
                  - id
                properties:
                  id:
                    type: string
                  content_parts:
                    type: array
                    items:
                      oneOf:
                        - $ref: '#/components/schemas/ChatCompletionRequestMessageContentPartText'
                        - $ref: '#/components/schemas/ChatCompletionRequestMessageContentPartImage'
        first_id:
          type: string
        last_id:
          type: string
        has_more:
          type: boolean
"##,
    )
    .expect("generate inline allOf array item fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let list = find_struct(&types_rs, "ChatCompletionMessageList");
    assert_field(list, "data", "Vec<ChatCompletionMessageListDataItem>");

    let item = find_struct(&types_rs, "ChatCompletionMessageListDataItem");
    assert_eq!(
        field_names(item),
        ["role", "content", "id", "content_parts"]
    );
    assert_field(item, "role", "String");
    assert_field(item, "content", "String");
    assert_field(item, "id", "String");
    assert_field(
        item,
        "content_parts",
        "Option<Vec<ChatCompletionMessageListDataItemContentPartsItem>>",
    );
}

#[test]
fn generated_inline_all_of_array_items_decode_flattened_fields() {
    let files = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Messages API
  version: 1.0.0
paths:
  /messages:
    get:
      operationId: listMessages
      responses:
        '200':
          description: Messages
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ChatCompletionMessageList'
components:
  schemas:
    ChatCompletionRequestMessageContentPartText:
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
    ChatCompletionRequestMessageContentPartImage:
      type: object
      required:
        - type
        - image_url
      properties:
        type:
          type: string
          enum:
            - image_url
        image_url:
          type: string
    ChatCompletionResponseMessage:
      type: object
      required:
        - role
        - content
      properties:
        role:
          type: string
        content:
          type: string
    ChatCompletionMessageList:
      type: object
      required:
        - object
        - data
        - first_id
        - last_id
        - has_more
      properties:
        object:
          type: string
          enum:
            - list
        data:
          type: array
          items:
            allOf:
              - $ref: '#/components/schemas/ChatCompletionResponseMessage'
              - type: object
                required:
                  - id
                properties:
                  id:
                    type: string
                  content_parts:
                    type: array
                    items:
                      oneOf:
                        - $ref: '#/components/schemas/ChatCompletionRequestMessageContentPartText'
                        - $ref: '#/components/schemas/ChatCompletionRequestMessageContentPartImage'
        first_id:
          type: string
        last_id:
          type: string
        has_more:
          type: boolean
"##,
    )
    .expect("generate inline allOf runtime fixture");

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
    fn decodes_inline_all_of_array_items() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"object":"list","data":[{"role":"user","content":"hello","id":"chatcmpl-1-0","content_parts":[{"type":"text","text":"hello"}]}],"first_id":"chatcmpl-1-0","last_id":"chatcmpl-1-0","has_more":false}"#.to_vec(),
        };

        let decoded = decode_list_messages_response(response).expect("decoded response");
        match decoded {
            ListMessagesResponse::Ok(list) => {
                assert_eq!(list.data.len(), 1);
                let item = &list.data[0];
                assert_eq!(item.role, "user");
                assert_eq!(item.content, "hello");
                assert_eq!(item.id, "chatcmpl-1-0");
                assert_eq!(item.content_parts.as_ref().expect("content parts").len(), 1);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "inline allOf array item generated crate tests",
    );
}

#[test]
fn all_of_rejects_duplicate_properties() {
    let err = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Child API
  version: 1.0.0
paths:
  /child:
    get:
      operationId: getChild
      responses:
        '200':
          description: Child
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Child'
components:
  schemas:
    Base:
      type: object
      properties:
        id:
          type: string
    Child:
      allOf:
        - $ref: '#/components/schemas/Base'
        - type: object
          properties:
            id:
              type: string
"##,
    )
    .expect_err("duplicate allOf properties are rejected");

    match err {
        Error::Validation(ValidationError::DuplicateAllOfProperty { context, property }) => {
            assert_eq!(context, "schema `Child`");
            assert_eq!(property, "id");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn all_of_rejects_primitive_branches() {
    let err = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Child API
  version: 1.0.0
paths:
  /child:
    get:
      operationId: getChild
      responses:
        '200':
          description: Child
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Child'
components:
  schemas:
    Child:
      allOf:
        - type: string
"##,
    )
    .expect_err("primitive allOf branches are rejected");

    match err {
        Error::Validation(ValidationError::UnsupportedAllOfBranch { context, index }) => {
            assert_eq!(context, "schema `Child`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn all_of_rejects_recursive_component_cycles() {
    let err = satay_codegen::generate(
        r##"
openapi: 3.1.0
info:
  title: Child API
  version: 1.0.0
paths:
  /node:
    get:
      operationId: getNode
      responses:
        '200':
          description: Node
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/A'
components:
  schemas:
    A:
      allOf:
        - $ref: '#/components/schemas/B'
    B:
      allOf:
        - $ref: '#/components/schemas/A'
"##,
    )
    .expect_err("recursive allOf cycles are rejected");

    match err {
        Error::Validation(ValidationError::RecursiveAllOf { context, schema }) => {
            assert!(context == "schema `A`" || context == "schema `B`");
            assert!(schema == "A" || schema == "B");
        }
        other => panic!("unexpected error: {other}"),
    }
}
