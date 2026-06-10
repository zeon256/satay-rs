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
fn rejects_unsupported_all_of_schema_forms_explicitly() {
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
"##,
    );
    match err {
        ValidationError::UnsupportedComposition { context, keyword } => {
            assert_eq!(context, "property `Parent.child`");
            assert_eq!(keyword, "allOf");
        }
        other => panic!("unexpected error: {other}"),
    }

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
