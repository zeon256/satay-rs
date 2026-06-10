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
fn rejects_any_of_with_inline_primitive_branch() {
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
fn rejects_one_of_without_discriminator() {
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
    Organization:
      type: object
      properties:
        id:
          type: string
    Broken:
      oneOf:
        - $ref: '#/components/schemas/User'
        - $ref: '#/components/schemas/Organization'
"##,
    );
    match err {
        ValidationError::UnsupportedComposition { context, keyword } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(keyword, "oneOf");
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
fn rejects_discriminator_property_conflict_with_branch_field() {
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
        ValidationError::DiscriminatorPropertyConflict {
            context,
            schema,
            property,
        } => {
            assert_eq!(context, "schema `Pet`");
            assert_eq!(schema, "Dog");
            assert_eq!(property, "kind");
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
