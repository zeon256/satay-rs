use super::*;

#[test]
fn lifts_inline_constraints_into_generated_types() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
        - name: tag
          in: query
          schema:
            type: array
            minItems: 1
            items:
              type: string
              minLength: 2
      responses:
        '204':
          description: No content
components:
  schemas:
    Age:
      type: integer
      format: int32
      minimum: 0
      maximum: 130
    DisplayName:
      type: [string, "null"]
      minLength: 1
"#,
    );

    let age = component(&api, "Age");
    match &age.kind {
        ComponentKind::Nutype(constrained) => {
            assert_eq!(constrained.rust_name, "Age");
            assert_eq!(constrained.inner, TypeRef::Integer(IntegerType::I32));
            match &constrained.validation {
                Validation::Integer { minimum, maximum } => {
                    assert_eq!(
                        minimum,
                        &Some(IntegerLimit {
                            value: 0,
                            exclusive: false,
                        })
                    );
                    assert_eq!(
                        maximum,
                        &Some(IntegerLimit {
                            value: 130,
                            exclusive: false,
                        })
                    );
                }
                other => panic!("expected Age integer validation, got {other:?}"),
            }
        }
        other => panic!("expected Age nutype, got {other:?}"),
    }

    let display_name = component(&api, "DisplayName");
    match &display_name.kind {
        ComponentKind::Alias(TypeRef::Option(inner)) => match inner.as_ref() {
            TypeRef::Constrained { rust_name, inner } => {
                assert_eq!(rust_name, "DisplayNameValue");
                assert_eq!(inner.as_ref(), &TypeRef::String);
            }
            other => panic!("expected constrained nullable DisplayName, got {other:?}"),
        },
        other => panic!("expected DisplayName nullable alias, got {other:?}"),
    }

    let generated_names = api
        .constrained_types
        .iter()
        .map(|constrained| constrained.rust_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        generated_names,
        [
            "DisplayNameValue",
            "GetUserTagParameterItem",
            "GetUserTagParameter",
        ]
    );

    match &api.constrained_types[0].validation {
        Validation::String {
            min_length,
            max_length,
            pattern,
        } => {
            assert_eq!(*min_length, Some(1));
            assert_eq!(*max_length, None);
            assert_eq!(*pattern, None);
        }
        other => panic!("expected DisplayNameValue string validation, got {other:?}"),
    }

    match &api.constrained_types[1].validation {
        Validation::String {
            min_length,
            max_length,
            pattern,
        } => {
            assert_eq!(*min_length, Some(2));
            assert_eq!(*max_length, None);
            assert_eq!(*pattern, None);
        }
        other => panic!("expected tag item string validation, got {other:?}"),
    }

    match &api.constrained_types[2].validation {
        Validation::Array {
            min_items,
            max_items,
        } => {
            assert_eq!(*min_items, Some(1));
            assert_eq!(*max_items, None);
        }
        other => panic!("expected tag array validation, got {other:?}"),
    }

    let operation = &api.operations[0];
    let tag = parameter(operation, "tag");
    match &tag.ty {
        TypeRef::Constrained { rust_name, inner } => {
            assert_eq!(rust_name, "GetUserTagParameter");
            match inner.as_ref() {
                TypeRef::Array(item) => match item.as_ref() {
                    TypeRef::Constrained { rust_name, inner } => {
                        assert_eq!(rust_name, "GetUserTagParameterItem");
                        assert_eq!(inner.as_ref(), &TypeRef::String);
                    }
                    other => panic!("expected constrained tag item, got {other:?}"),
                },
                other => panic!("expected constrained tag array, got {other:?}"),
            }
        }
        other => panic!("expected constrained tag parameter, got {other:?}"),
    }
}

#[test]
fn rejects_inverted_string_length_bounds() {
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
      type: string
      minLength: 4
      maxLength: 2
"#,
    );
    match err {
        ValidationError::InvalidStringLengthBounds {
            context,
            min_length,
            max_length,
        } => {
            assert_eq!(context, "schema `Broken`");
            assert_eq!(min_length, 4);
            assert_eq!(max_length, 2);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_empty_integer_bounds() {
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
      type: integer
      format: int32
      exclusiveMinimum: 5
      maximum: 5
"#,
    );
    match err {
        ValidationError::EmptyIntegerBounds { context } => {
            assert_eq!(context, "schema `Broken`");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_empty_number_bounds() {
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
      type: number
      exclusiveMinimum: 5
      exclusiveMaximum: 5
"#,
    );
    match err {
        ValidationError::EmptyNumberBounds { context } => {
            assert_eq!(context, "schema `Broken`");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn parses_uint32_and_uint64_integer_formats() {
    let api = parse_valid(
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
    Index:
      type: integer
      format: uint32
    BigCount:
      type: integer
      format: uint64
    FlooredIndex:
      type: integer
      format: uint32
      minimum: 5
    BoundedIndex:
      type: integer
      format: uint32
      minimum: 0
      maximum: 10
"#,
    );

    match &component(&api, "Index").kind {
        ComponentKind::Alias(ty) => {
            assert_eq!(ty, &TypeRef::Integer(IntegerType::U32));
        }
        other => panic!("expected Index alias, got {other:?}"),
    }

    match &component(&api, "BigCount").kind {
        ComponentKind::Alias(ty) => {
            assert_eq!(ty, &TypeRef::Integer(IntegerType::U64));
        }
        other => panic!("expected BigCount alias, got {other:?}"),
    }

    // Explicit format keeps the u32 base (no single-bound widening to u64)
    // while the bound becomes a validation newtype.
    match &component(&api, "FlooredIndex").kind {
        ComponentKind::Nutype(constrained) => {
            assert_eq!(constrained.inner, TypeRef::Integer(IntegerType::U32));
            match &constrained.validation {
                Validation::Integer { minimum, maximum } => {
                    assert_eq!(
                        minimum,
                        &Some(IntegerLimit {
                            value: 5,
                            exclusive: false,
                        })
                    );
                    assert_eq!(maximum, &None);
                }
                other => panic!("expected FlooredIndex integer validation, got {other:?}"),
            }
        }
        other => panic!("expected FlooredIndex nutype, got {other:?}"),
    }

    // Explicit format wins over dual-bound narrowing: the base stays u32.
    match &component(&api, "BoundedIndex").kind {
        ComponentKind::Nutype(constrained) => {
            assert_eq!(constrained.inner, TypeRef::Integer(IntegerType::U32));
            match &constrained.validation {
                Validation::Integer { minimum, maximum } => {
                    assert_eq!(minimum, &None);
                    assert_eq!(
                        maximum,
                        &Some(IntegerLimit {
                            value: 10,
                            exclusive: false,
                        })
                    );
                }
                other => panic!("expected BoundedIndex integer validation, got {other:?}"),
            }
        }
        other => panic!("expected BoundedIndex nutype, got {other:?}"),
    }
}

#[test]
fn rejects_unknown_integer_format_uint8() {
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
    Tiny:
      type: integer
      format: uint8
"#,
    );

    match err {
        ValidationError::UnsupportedIntegerFormat { format, .. } => {
            assert_eq!(format, "uint8");
        }
        other => panic!("unexpected error: {other}"),
    }
}
