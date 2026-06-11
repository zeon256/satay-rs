use super::*;

#[test]
fn parses_all_of_component_and_inline_branches_into_ir() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
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
"#,
    );

    let child = component(&api, "Child");
    assert_eq!(child.description.as_deref(), Some("A flattened child."));
    match &child.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(fields.len(), 4);
            assert_eq!(field(fields, "id").rust_name, "id");
            assert!(field(fields, "id").required);
            assert_eq!(field(fields, "tag").rust_name, "tag");
            assert!(field(fields, "tag").required);
            assert_eq!(field(fields, "name").rust_name, "name");
            assert!(field(fields, "name").required);
            assert_eq!(field(fields, "nickname").rust_name, "nickname");
            assert!(!field(fields, "nickname").required);
        }
        other => panic!("expected Child struct, got {other:?}"),
    }

    let decorated = component(&api, "Decorated");
    match &decorated.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(field(fields, "id").rust_name, "id");
            assert_eq!(field(fields, "tag").rust_name, "tag");
        }
        other => panic!("expected Decorated struct, got {other:?}"),
    }

    assert_eq!(
        api.operations[0].responses[0].body,
        Some(TypeRef::Named("Child".to_owned()))
    );
}

#[test]
fn parses_inline_all_of_array_items_into_generated_struct_ir() {
    let api = parse_valid(
        r##"
openapi: 3.1.0
info:
  title: Test API
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
        first_id:
          type: string
        last_id:
          type: string
        has_more:
          type: boolean
"##,
    );

    let list = component(&api, "ChatCompletionMessageList");
    match &list.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(
                field(fields, "data").ty,
                TypeRef::Array(Box::new(TypeRef::Named(
                    "ChatCompletionMessageListDataItem".to_owned()
                )))
            );
        }
        other => panic!("expected ChatCompletionMessageList struct, got {other:?}"),
    }

    let item = component(&api, "ChatCompletionMessageListDataItem");
    match &item.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(fields.len(), 3);
            assert_eq!(field(fields, "role").ty, TypeRef::String);
            assert_eq!(field(fields, "content").ty, TypeRef::String);
            assert_eq!(field(fields, "id").ty, TypeRef::String);
            assert!(field(fields, "id").required);
        }
        other => panic!("expected generated inline item struct, got {other:?}"),
    }
}

#[test]
fn rejects_all_of_with_sibling_properties_keyword() {
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
    Base:
      type: object
      properties:
        id:
          type: string
    Broken:
      allOf:
        - $ref: '#/components/schemas/Base'
      properties:
        extra:
          type: string
"##,
    );
    match err {
        ValidationError::UnsupportedAllOfSiblingKeyword { context, keyword } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "properties");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_all_of_with_duplicate_properties_across_branches() {
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
    Base:
      type: object
      properties:
        id:
          type: string
    Broken:
      allOf:
        - $ref: '#/components/schemas/Base'
        - type: object
          properties:
            id:
              type: string
"##,
    );
    match err {
        ValidationError::DuplicateAllOfProperty { context, property } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(property, "id");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_all_of_with_primitive_branch() {
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
      allOf:
        - type: string
"##,
    );
    match err {
        ValidationError::UnsupportedAllOfBranch { context, index } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_all_of_with_nested_all_of_branch() {
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
      allOf:
        - type: object
          allOf:
            - type: object
              properties:
                id:
                  type: string
"##,
    );
    match err {
        ValidationError::UnsupportedAllOfBranch { context, index } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_all_of_branch_referencing_any_of_union() {
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
    Organization:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    Union:
      anyOf:
        - $ref: '#/components/schemas/User'
        - $ref: '#/components/schemas/Organization'
    Broken:
      allOf:
        - $ref: '#/components/schemas/Union'
"##,
    );
    match err {
        ValidationError::UnsupportedAllOfBranch { context, index } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_mutually_recursive_all_of_components() {
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
      allOf:
        - $ref: '#/components/schemas/B'
    B:
      allOf:
        - $ref: '#/components/schemas/A'
"##,
    );
    match err {
        ValidationError::RecursiveAllOf { context, schema } => {
            assert!(context == "schema `A`" || context == "schema `B`");
            assert!(schema == "A" || schema == "B");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_self_recursive_inline_all_of_property() {
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
    Node:
      type: object
      properties:
        child:
          allOf:
            - $ref: '#/components/schemas/Node'
"##,
    );
    match err {
        ValidationError::RecursiveAllOf { context, schema } => {
            assert_eq!(context, "schema `Node`");
            assert_eq!(schema, "Node");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_mutually_recursive_inline_all_of_properties() {
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
          allOf:
            - $ref: '#/components/schemas/B'
    B:
      type: object
      properties:
        parent:
          allOf:
            - $ref: '#/components/schemas/A'
"##,
    );
    match err {
        ValidationError::RecursiveAllOf { context, schema } => {
            assert!(context == "schema `A`" || context == "schema `B`");
            assert!(schema == "A" || schema == "B");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_all_of_in_parameter_schemas() {
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
            allOf:
              - type: object
                properties:
                  id:
                    type: string
      responses:
        '204':
          description: No content
"##,
    );
    match err {
        ValidationError::UnsupportedComposition { context, keyword } => {
            assert_eq!(context, "parameter `filter`");
            assert_eq!(keyword, "allOf");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn parses_all_of_in_inline_property_schemas() {
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
    Parent:
      type: object
      properties:
        child:
          allOf:
            - type: object
              properties:
                id:
                  type: string
"##,
    );

    let parent = component(&api, "Parent");
    match &parent.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(
                field(fields, "child").ty,
                TypeRef::Named("ParentChild".to_owned())
            );
        }
        other => panic!("expected Parent struct, got {other:?}"),
    }

    let child = component(&api, "ParentChild");
    match &child.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(fields.len(), 1);
            assert_eq!(field(fields, "id").ty, TypeRef::String);
        }
        other => panic!("expected ParentChild struct, got {other:?}"),
    }
}

#[test]
fn rejects_inline_all_of_with_duplicate_properties() {
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
    Parent:
      type: object
      properties:
        child:
          allOf:
            - type: object
              properties:
                id:
                  type: string
            - type: object
              properties:
                id:
                  type: string
"##,
    );
    match err {
        ValidationError::DuplicateAllOfProperty { context, property } => {
            assert_eq!(context, "property `Parent.child`");
            assert_eq!(property, "id");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_inline_all_of_with_primitive_branch() {
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
    Parent:
      type: object
      properties:
        children:
          type: array
          items:
            allOf:
              - type: string
"##,
    );
    match err {
        ValidationError::UnsupportedAllOfBranch { context, index } => {
            assert_eq!(context, "property `Parent.children` items");
            assert_eq!(index, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_inline_all_of_with_sibling_properties_keyword() {
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
    Parent:
      type: object
      properties:
        child:
          allOf:
            - type: object
              properties:
                id:
                  type: string
          properties:
            extra:
              type: string
"##,
    );
    match err {
        ValidationError::UnsupportedAllOfSiblingKeyword { context, keyword } => {
            assert_eq!(context, "property `Parent.child`");
            assert_eq!(keyword, "properties");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_empty_all_of() {
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
      allOf: []
"##,
    );
    match err {
        ValidationError::EmptyAnyOf { context } => {
            assert_eq!(context, "schema `Empty`");
        }
        other => panic!("unexpected error: {other}"),
    }
}
