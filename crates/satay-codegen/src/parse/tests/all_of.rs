use satay_ir::{CompositionKind, TypeExpr};

use super::ast::*;
use super::*;

#[test]
fn parses_all_of_component_and_inline_branches_into_ir() {
    let spec = r#"
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
"#;

    let semantic = normalize_spec(spec);
    let (_, child_ir) = semantic
        .definitions()
        .find(|(_, d)| d.source_name == "Child")
        .unwrap();

    let TypeExpr::Composition(composition) = &child_ir.schema.ty else {
        panic!("allOf tree must not be flattened")
    };

    assert_eq!(composition.kind, CompositionKind::AllOf);
    let TypeExpr::Ref(decorated_id) = composition.branches[0].ty else {
        panic!("component branch identity")
    };

    assert_eq!(
        semantic.definition(decorated_id).unwrap().source_name,
        "Decorated"
    );

    let TypeExpr::Object(inline) = &composition.branches[1].ty else {
        panic!("inline object branch")
    };

    assert_eq!(
        inline
            .properties
            .iter()
            .map(|p| p.wire_name.as_str())
            .collect::<Vec<_>>(),
        ["name", "nickname"]
    );

    assert_eq!(
        composition.branches[1]
            .annotations
            .source
            .as_ref()
            .unwrap()
            .pointer,
        "/components/schemas/Child/allOf/1"
    );

    // NOTE: the private Rust model is gone; the flattening facts are now
    // asserted on the generated Rust output. Requiredness shows up as the
    // plain `S` field type, optionality as `Option<S>`.
    let files = generate_valid(spec);
    let types = parse_rust(file(&files, "types.rs"));

    let child = find_struct(&types, "Child");
    assert_doc(&child.attrs, "A flattened child.");
    assert_eq!(field_names(child), ["id", "tag", "name", "nickname"]);
    assert_field(child, "id", "S");
    assert_field(child, "tag", "S");
    assert_field(child, "name", "S");
    assert_field(child, "nickname", "Option<S>");

    let decorated = find_struct(&types, "Decorated");
    assert_eq!(field_names(decorated), ["id", "tag"]);

    // The operation response body decodes into the flattened `Child` type.
    let parts = parse_rust(file(&files, "get_child/parts.rs"));
    let response = find_enum(&parts, "GetChildResponse");
    let ok = variant(response, "Ok");
    assert!(
        contains_tokens(ok, "Child<S>"),
        "response body must be the flattened `Child` type, got `{}`",
        norm(ok)
    );
}

#[test]
fn rejects_x_satay_on_all_of_reference_branch() {
    let err = parse_invalid(
        r##"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
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
          x-satay:
            integer-type: auto
"##,
    );

    assert!(matches!(
        err,
        ValidationError::UnsupportedRefSiblingKeyword { context, keyword }
            if context == "schema `Broken`.allOf[0]"
                && keyword == "x-satay.integer-type"
    ));
}

#[test]
fn parses_inline_all_of_array_items_into_generated_struct_ir() {
    let spec = r##"
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
"##;
    let files = generate_valid(spec);

    // NOTE: the private model's `TypeRef::Array(Named(...))` fact maps to the
    // generated `Vec<...DataItem<S>>` field type on the list struct.
    let types = parse_rust(file(&files, "types.rs"));
    let list = find_struct(&types, "ChatCompletionMessageList");
    assert_field(list, "data", "Vec<ChatCompletionMessageListDataItem<S>>");

    let item = find_struct(&types, "ChatCompletionMessageListDataItem");
    assert_eq!(field_names(item), ["role", "content", "id"]);
    assert_field(item, "role", "S");
    assert_field(item, "content", "S");
    assert_field(item, "id", "S");
}

#[test]
fn rejects_all_of_with_sibling_properties_keyword() {
    let err = parse_invalid(
        r#"
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
"#,
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
fn rejects_all_of_duplicate_even_when_first_property_is_ignored() {
    let err = parse_invalid(
        r#"
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
          x-satay:
            ignore: true
    Broken:
      allOf:
        - $ref: '#/components/schemas/Base'
        - type: object
          properties:
            id:
              type: string
"#,
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
        r#"
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
"#,
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
        r#"
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
"#,
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
        r#"
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
"#,
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
        r#"
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
"#,
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
        r#"
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
"#,
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
        r#"
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
"#,
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
fn rejects_inline_all_of_cycle_through_discriminator_branch() {
    let err = parse_invalid(
        r#"
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
    C:
      type: object
      properties:
        u:
          oneOf:
            - $ref: '#/components/schemas/C2'
            - $ref: '#/components/schemas/C3'
          discriminator:
            propertyName: kind
    C2:
      type: object
      required: [kind]
      properties:
        kind:
          type: string
        next:
          allOf:
            - $ref: '#/components/schemas/C'
    C3:
      type: object
      required: [kind]
      properties:
        kind:
          type: string
"#,
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
fn rejects_all_of_in_parameter_schemas() {
    let err = parse_invalid(
        r#"
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
"#,
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
    let files = generate_valid(
        r#"
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
"#,
    );

    let types = parse_rust(file(&files, "types.rs"));

    let parent = find_struct(&types, "Parent");
    assert_field(parent, "child", "Option<ParentChild<S>>");

    let child = find_struct(&types, "ParentChild");
    assert_eq!(field_names(child), ["id"]);
    assert_field(child, "id", "Option<S>");
}

#[test]
fn rejects_inline_all_of_with_duplicate_properties() {
    let err = parse_invalid(
        r#"
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
"#,
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
        r#"
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
"#,
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
        r#"
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
"#,
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
fn parses_empty_all_of_as_json_value() {
    let files = generate_valid(
        r#"
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
"#,
    );

    // NOTE: the private model's `ComponentKind::Alias(JsonValue)` maps to a
    // generated type alias over the runtime JSON value.
    let types = parse_rust(file(&files, "types.rs"));
    let empty = find_type_alias(&types, "Empty");
    assert_eq!(norm(&empty.ty), norm_str("satay_runtime::JsonValue"));
}

#[test]
fn unwraps_annotation_only_all_of_ref_wrapper_property_to_named_type() {
    let files = generate_valid(
        r#"
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
    Relationship:
      type: string
      enum:
        - friend
        - family
    Person:
      type: object
      properties:
        relationship:
          description: How they are related.
          allOf:
            - $ref: '#/components/schemas/Relationship'
"#,
    );

    let types = parse_rust(file(&files, "types.rs"));

    let person = find_struct(&types, "Person");
    assert_eq!(field_names(person), ["relationship"]);
    // Unwrapped to the referenced named type; the property annotation (the
    // description) is carried onto the generated field.
    assert_field(person, "relationship", "Option<Relationship>");
    assert_doc(
        &field(person, "relationship").attrs,
        "How they are related.",
    );
}

#[test]
fn wrapped_ref_property_without_description_uses_referenced_description() {
    let files = generate_valid(
        r#"
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
    Relationship:
      description: The relationship kind.
      type: string
      enum:
        - friend
        - family
    Person:
      type: object
      properties:
        relationship:
          title: Relationship
          allOf:
            - $ref: '#/components/schemas/Relationship'
"#,
    );

    let types = parse_rust(file(&files, "types.rs"));

    let person = find_struct(&types, "Person");
    assert_doc(
        &field(person, "relationship").attrs,
        "The relationship kind.",
    );
}

#[test]
fn unwraps_annotation_only_all_of_ref_wrapper_property_targeting_union() {
    let files = generate_valid(
        r#"
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
        - name
      properties:
        name:
          type: string
    Choice:
      anyOf:
        - $ref: '#/components/schemas/User'
        - $ref: '#/components/schemas/Organization'
    Holder:
      type: object
      properties:
        choice:
          description: Pick one.
          allOf:
            - $ref: '#/components/schemas/Choice'
"#,
    );

    let types = parse_rust(file(&files, "types.rs"));

    let holder = find_struct(&types, "Holder");
    assert_eq!(field_names(holder), ["choice"]);
    assert_field(holder, "choice", "Option<Choice<S>>");
    assert_doc(&field(holder, "choice").attrs, "Pick one.");
}

#[test]
fn wrapped_struct_ref_property_still_flattens_to_inline_struct() {
    let files = generate_valid(
        r#"
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
      required:
        - id
      properties:
        id:
          type: string
    Holder:
      type: object
      properties:
        child:
          description: Annotated child.
          allOf:
            - $ref: '#/components/schemas/Base'
"#,
    );

    let types = parse_rust(file(&files, "types.rs"));

    let holder = find_struct(&types, "Holder");
    assert_field(holder, "child", "Option<HolderChild<S>>");

    // Struct-target wrappers keep the flattening carve-out: the referenced
    // object's properties land on a dedicated named struct.
    let child = find_struct(&types, "HolderChild");
    assert_eq!(field_names(child), ["id"]);
    assert_field(child, "id", "S");
}

#[test]
fn flattens_all_of_branch_with_additional_properties_false_and_properties() {
    let files = generate_valid(
        r#"
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
    Target:
      type: object
      additionalProperties: false
      required:
        - enabled
      properties:
        enabled:
          type: boolean
    Holder:
      type: object
      properties:
        config:
          description: Annotated config.
          allOf:
            - $ref: '#/components/schemas/Target'
"#,
    );

    let types = parse_rust(file(&files, "types.rs"));

    let holder = find_struct(&types, "Holder");
    assert_field(holder, "config", "Option<HolderConfig>");

    let config = find_struct(&types, "HolderConfig");
    assert_eq!(field_names(config), ["enabled"]);
    assert_field(config, "enabled", "bool");
}

#[test]
fn rejects_all_of_branch_with_additional_properties_on_propertyless_object() {
    let err = parse_invalid(
        r#"
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
        config:
          description: Annotated config.
          allOf:
            - type: object
              additionalProperties: false
"#,
    );

    match err {
        ValidationError::UnsupportedAllOfSiblingKeyword { keyword, .. } => {
            assert_eq!(keyword, "additionalProperties");
        }
        other => panic!("unexpected error: {other}"),
    }
}
