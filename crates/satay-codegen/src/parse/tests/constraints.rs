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
            assert_eq!(constrained.inner, TypeRef::Integer(IntegerType::U8));
            match &constrained.validation {
                Validation::Integer { minimum, maximum } => {
                    assert_eq!(minimum, &None);
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
fn rejects_invalid_validation_bounds_before_rendering() {
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
