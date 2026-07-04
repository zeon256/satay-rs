use super::*;

#[test]
fn lowers_inline_constrained_enum_and_range_schemas_to_ir() {
    let api = parse_valid(INLINE_CONSTRAINED_ENUM_RANGE);

    let search = component(&api, "Search");
    match &search.kind {
        ComponentKind::Struct(fields) => {
            match &field(fields, "code").ty {
                TypeRef::Constrained { rust_name, inner } => {
                    assert_eq!(rust_name, "SearchCode");
                    assert_eq!(inner.as_ref(), &TypeRef::String);
                }
                other => panic!("expected constrained code field, got {other:?}"),
            }
            assert_eq!(
                field(fields, "state").ty,
                TypeRef::Named("SearchState".to_owned())
            );
            assert_eq!(
                field(fields, "window").ty,
                TypeRef::Range(RangeTypeRef {
                    rust_name: "SearchWindow".to_owned(),
                    scalar: RangeScalar::Integer(IntegerType::U8),
                })
            );
        }
        other => panic!("expected Search struct, got {other:?}"),
    }

    let state = component(&api, "SearchState");
    match &state.kind {
        ComponentKind::Enum(enum_) => {
            let variants = &enum_.variants;
            assert_eq!(variants[0].rust_name, "Open");
            assert_eq!(variants[1].rust_name, "Closed");
            assert_eq!(enum_.fallback, EnumFallback::None);
        }
        other => panic!("expected SearchState enum, got {other:?}"),
    }

    let window = component(&api, "SearchWindow");
    match &window.kind {
        ComponentKind::Range(range) => {
            assert_eq!(range.scalar, RangeScalar::Integer(IntegerType::U8));
        }
        other => panic!("expected SearchWindow range, got {other:?}"),
    }

    assert_eq!(api.constrained_types.len(), 1);
    assert_eq!(api.constrained_types[0].rust_name, "SearchCode");
    match &api.constrained_types[0].validation {
        Validation::String {
            min_length,
            max_length,
            pattern,
        } => {
            assert_eq!(*min_length, Some(2));
            assert_eq!(*max_length, Some(8));
            assert_eq!(*pattern, None);
        }
        other => panic!("expected string validation, got {other:?}"),
    }
}

#[test]
fn parses_components_operations_and_json_media_types_into_ir() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
servers:
  - url: https://api.example.test/v1
paths:
  /users/{userId}:
    parameters:
      - name: userId
        in: path
        required: true
        schema:
          type: string
      - name: body
        in: query
        schema:
          type: boolean
    get:
      operationId: getUser
      parameters:
        - name: body
          in: query
          required: false
          schema:
            type: integer
            format: int32
        - name: includeDetails
          in: query
          required: true
          schema:
            type: boolean
      requestBody:
        required: true
        content:
          application/vnd.acme.user+json; charset=utf-8:
            schema:
              $ref: '#/components/schemas/UpdateUserRequest'
      responses:
        '404':
          description: Missing
        '200':
          description: Found
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
components:
  securitySchemes:
    accountKeyAuth:
      type: apiKey
      in: header
      name: AccountKey
    queryKeyAuth:
      type: apiKey
      in: query
      name: api_key
    bearerAuth:
      type: http
      scheme: bearer
  schemas:
    UpdateUserRequest:
      type: object
      required:
        - name
      properties:
        name:
          type: string
    User:
      type: object
      required:
        - id
        - status
      properties:
        id:
          type: string
        status:
          type: string
          enum:
            - active
            - suspended
        age:
          type: integer
          format: int64
"#,
    );

    assert_eq!(api.components.len(), 3);
    assert!(api.constrained_types.is_empty());
    assert_eq!(api.server_url, "https://api.example.test/v1");
    assert_eq!(api.api_key_security_schemes.len(), 2);
    assert_eq!(
        api.api_key_security_schemes[0].location,
        ApiKeyLocation::Header
    );
    assert_eq!(api.api_key_security_schemes[0].wire_name, "AccountKey");
    assert_eq!(api.api_key_security_schemes[0].rust_name, "account_key");
    assert_eq!(
        api.api_key_security_schemes[1].location,
        ApiKeyLocation::Query
    );
    assert_eq!(api.api_key_security_schemes[1].wire_name, "api_key");
    assert_eq!(api.api_key_security_schemes[1].rust_name, "api_key");

    let update_user_request = component(&api, "UpdateUserRequest");
    match &update_user_request.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(fields.len(), 1);
            let name = field(fields, "name");
            assert_eq!(name.rust_name, "name");
            assert_eq!(name.ty, TypeRef::String);
            assert!(name.required);
        }
        other => panic!("expected UpdateUserRequest struct, got {other:?}"),
    }

    let user_status = component(&api, "UserStatus");
    match &user_status.kind {
        ComponentKind::Enum(enum_) => {
            let variants = &enum_.variants;
            assert_eq!(variants.len(), 2);
            assert_eq!(variants[0].wire_name, "active");
            assert_eq!(variants[0].rust_name, "Active");
            assert_eq!(variants[1].wire_name, "suspended");
            assert_eq!(variants[1].rust_name, "Suspended");
            assert_eq!(enum_.fallback, EnumFallback::None);
        }
        other => panic!("expected UserStatus enum, got {other:?}"),
    }

    let user = component(&api, "User");
    match &user.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(fields.len(), 3);

            let id = field(fields, "id");
            assert_eq!(id.ty, TypeRef::String);
            assert!(id.required);

            let status = field(fields, "status");
            assert_eq!(status.ty, TypeRef::Named("UserStatus".to_owned()));
            assert!(status.required);

            let age = field(fields, "age");
            assert_eq!(age.ty, TypeRef::Integer(IntegerType::I64));
            assert!(!age.required);
        }
        other => panic!("expected User struct, got {other:?}"),
    }

    assert_eq!(api.operations.len(), 1);
    let operation = &api.operations[0];
    assert_eq!(operation.fn_name, "get_user");
    assert_eq!(operation.input_name, "GetUserInput");
    assert_eq!(operation.response_name, "GetUserResponse");
    assert_eq!(operation.method, HttpMethod::Get);
    assert_eq!(operation.path, "/users/{userId}");
    assert_eq!(operation.path_segments.len(), 2);
    assert_literal_segment(&operation.path_segments[0], "/users/");
    assert_parameter_segment(&operation.path_segments[1], "userId");

    assert_eq!(operation.parameters.len(), 3);
    let user_id = parameter(operation, "userId");
    assert_eq!(user_id.location, ParameterLocation::Path);
    assert_eq!(user_id.rust_name, "user_id");
    assert_eq!(user_id.ty, TypeRef::String);
    assert!(user_id.required);

    let body = parameter(operation, "body");
    assert_eq!(body.location, ParameterLocation::Query);
    assert_eq!(body.rust_name, "body");
    assert_eq!(body.ty, TypeRef::Integer(IntegerType::I32));
    assert!(!body.required);

    let include_details = parameter(operation, "includeDetails");
    assert_eq!(include_details.location, ParameterLocation::Query);
    assert_eq!(include_details.rust_name, "include_details");
    assert_eq!(include_details.ty, TypeRef::Bool);
    assert!(include_details.required);

    let request_body = operation.request_body.as_ref().expect("request body");
    assert_eq!(request_body.field_name, "body_2");
    assert_eq!(
        request_body.content_type,
        "application/vnd.acme.user+json; charset=utf-8"
    );
    assert_eq!(
        request_body.ty,
        TypeRef::Named("UpdateUserRequest".to_owned())
    );
    assert!(request_body.required);

    assert_eq!(operation.responses.len(), 2);
    assert_eq!(operation.responses[0].status, 200);
    assert_eq!(operation.responses[0].variant_name, "Ok");
    assert_eq!(
        operation.responses[0].body,
        Some(TypeRef::Named("User".to_owned()))
    );
    assert_eq!(operation.responses[1].status, 404);
    assert_eq!(operation.responses[1].variant_name, "NotFound");
    assert_eq!(operation.responses[1].body, None);
}

#[test]
fn lowers_alias_refs_before_rendering() {
    let api = parse_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /reading:
    get:
      operationId: getReading
      parameters:
        - name: readingId
          in: query
          required: true
          schema:
            $ref: '#/components/schemas/ReadingId'
      responses:
        '200':
          description: Reading
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Reading'
components:
  schemas:
    Reading:
      type: object
      required:
        - id
        - nickname
      properties:
        id:
          $ref: '#/components/schemas/ReadingId'
        nickname:
          $ref: '#/components/schemas/OptionalName'
    ReadingId:
      type: string
      x-satay:
        parse-as: u32
    OptionalName:
      type: [string, "null"]
"#,
    );

    let reading = component(&api, "Reading");
    match &reading.kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(field(fields, "id").ty, TypeRef::ParsedString(ParseAs::U32));
            assert_eq!(
                field(fields, "nickname").ty,
                TypeRef::Option(Box::new(TypeRef::String))
            );
        }
        other => panic!("expected Reading struct, got {other:?}"),
    }

    let reading_id = parameter(&api.operations[0], "readingId");
    assert_eq!(reading_id.ty, TypeRef::ParsedString(ParseAs::U32));
}

#[test]
fn infers_and_overrides_integer_types() {
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
    Direction:
      type: integer
      format: int32
      minimum: 1
      maximum: 2
    Byte:
      type: integer
      format: int64
      minimum: 0
      maximum: 255
    LegacyDirection:
      type: integer
      format: int32
      minimum: 1
      maximum: 2
      x-satay:
        integer-type: i32
    Unbounded:
      type: integer
      format: int32
"#,
    );

    let direction = component(&api, "Direction");
    match &direction.kind {
        ComponentKind::Nutype(constrained) => {
            assert_eq!(constrained.inner, TypeRef::Integer(IntegerType::U8));
        }
        other => panic!("expected Direction nutype, got {other:?}"),
    }

    let byte = component(&api, "Byte");
    match &byte.kind {
        ComponentKind::Alias(ty) => assert_eq!(ty, &TypeRef::Integer(IntegerType::U8)),
        other => panic!("expected Byte alias, got {other:?}"),
    }

    let legacy_direction = component(&api, "LegacyDirection");
    match &legacy_direction.kind {
        ComponentKind::Nutype(constrained) => {
            assert_eq!(constrained.inner, TypeRef::Integer(IntegerType::I32));
        }
        other => panic!("expected LegacyDirection nutype, got {other:?}"),
    }

    let unbounded = component(&api, "Unbounded");
    match &unbounded.kind {
        ComponentKind::Alias(ty) => assert_eq!(ty, &TypeRef::Integer(IntegerType::I32)),
        other => panic!("expected Unbounded alias, got {other:?}"),
    }
}

#[test]
fn parses_type_array_nullability_and_numeric_exclusive_bounds() {
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
    OptionalName:
      type: [string, "null"]
      minLength: 1
    Window:
      type: integer
      exclusiveMinimum: 0
      exclusiveMaximum: 10
"#,
    );

    match &component(&api, "OptionalName").kind {
        ComponentKind::Alias(TypeRef::Option(inner)) => match inner.as_ref() {
            TypeRef::Constrained { rust_name, inner } => {
                assert_eq!(rust_name, "OptionalNameValue");
                assert_eq!(inner.as_ref(), &TypeRef::String);
            }
            other => panic!("expected nullable constrained string, got {other:?}"),
        },
        other => panic!("expected OptionalName nullable alias, got {other:?}"),
    }

    match &component(&api, "Window").kind {
        ComponentKind::Nutype(constrained) => {
            assert_eq!(constrained.inner, TypeRef::Integer(IntegerType::U8));
            match &constrained.validation {
                Validation::Integer { minimum, maximum } => {
                    assert_eq!(
                        minimum,
                        &Some(IntegerLimit {
                            value: 0,
                            exclusive: true,
                        })
                    );
                    assert_eq!(
                        maximum,
                        &Some(IntegerLimit {
                            value: 10,
                            exclusive: true,
                        })
                    );
                }
                other => panic!("expected integer validation, got {other:?}"),
            }
        }
        other => panic!("expected Window nutype, got {other:?}"),
    }
}

#[test]
fn lowers_const_string_property_to_singleton_enum() {
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
    CacheControl:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          const: ephemeral
"#,
    );

    match &component(&api, "CacheControl").kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(
                field(fields, "type").ty,
                TypeRef::Named("CacheControlType".to_owned())
            );
        }
        other => panic!("expected CacheControl struct, got {other:?}"),
    }

    match &component(&api, "CacheControlType").kind {
        ComponentKind::Enum(enum_) => {
            assert_eq!(enum_.variants.len(), 1);
            assert_eq!(enum_.variants[0].wire_name, "ephemeral");
            assert_eq!(enum_.variants[0].rust_name, "Ephemeral");
            assert_eq!(enum_.fallback, EnumFallback::None);
        }
        other => panic!("expected CacheControlType enum, got {other:?}"),
    }
}

#[test]
fn lowers_single_ref_nullable_any_of_to_option_of_ref() {
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
    Profile:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    User:
      type: object
      properties:
        profile:
          anyOf:
            - $ref: '#/components/schemas/Profile'
            - type: 'null'
"##,
    );

    match &component(&api, "User").kind {
        ComponentKind::Struct(fields) => {
            assert_eq!(
                field(fields, "profile").ty,
                TypeRef::Option(Box::new(TypeRef::Named("Profile".to_owned())))
            );
        }
        other => panic!("expected User struct, got {other:?}"),
    }

    assert!(
        !api.components
            .iter()
            .any(|component| component.rust_name == "UserProfile"),
        "single-reference untagged union must not synthesize a wrapper component"
    );
}
