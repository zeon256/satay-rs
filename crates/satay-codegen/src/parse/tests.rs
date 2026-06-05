#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::error::ValidationError;
    use crate::model::{
        Api, ApiKeyLocation, Component, ComponentKind, Field, HttpMethod, IntegerLimit,
        IntegerType, Operation, Parameter, ParameterLocation, ParseAs, PathSegment, RangeScalar,
        RangeTypeRef, TypeRef, Validation,
    };
    use crate::parse::{parse_api, parse_document};

    const INLINE_CONSTRAINED_ENUM_RANGE: &str =
        include_str!("../../../../tests/fixtures/parse-inline-constrained-enum-range.yaml");

    fn parse_valid(spec: &str) -> Api {
        let document = parse_document(spec).expect("document parses");
        parse_api(&document).expect("OpenAPI validates")
    }

    fn parse_invalid(spec: &str) -> ValidationError {
        let document = parse_document(spec).expect("document parses");
        parse_api(&document).expect_err("OpenAPI must be rejected")
    }

    fn component<'a>(api: &'a Api, rust_name: &str) -> &'a Component {
        api.components
            .iter()
            .find(|component| component.rust_name == rust_name)
            .unwrap_or_else(|| panic!("missing component {rust_name}"))
    }

    fn field<'a>(fields: &'a [Field], wire_name: &str) -> &'a Field {
        fields
            .iter()
            .find(|field| field.wire_name == wire_name)
            .unwrap_or_else(|| panic!("missing field {wire_name}"))
    }

    fn parameter<'a>(operation: &'a Operation, wire_name: &str) -> &'a Parameter {
        operation
            .parameters
            .iter()
            .find(|parameter| parameter.wire_name == wire_name)
            .unwrap_or_else(|| panic!("missing parameter {wire_name}"))
    }

    fn api_key_rust_name<'a>(api: &'a Api, wire_name: &str) -> &'a str {
        api.api_key_security_schemes
            .iter()
            .find(|scheme| scheme.wire_name == wire_name)
            .map(|scheme| scheme.rust_name.as_str())
            .unwrap_or_else(|| panic!("missing API key security scheme {wire_name}"))
    }

    fn assert_literal_segment(segment: &PathSegment, expected: &str) {
        match segment {
            PathSegment::Literal(actual) => assert_eq!(actual, expected),
            other => panic!("expected literal path segment {expected:?}, got {other:?}"),
        }
    }

    fn assert_parameter_segment(segment: &PathSegment, expected: &str) {
        match segment {
            PathSegment::Parameter(actual) => assert_eq!(actual, expected),
            other => panic!("expected parameter path segment {expected:?}, got {other:?}"),
        }
    }

    #[test]
    fn deduplicates_parameter_field_names_after_identifier_sanitization() {
        let api = parse_valid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /users:
    get:
      operationId: listUsers
      parameters:
        - name: user-id
          in: query
          schema:
            type: string
        - name: user_id
          in: query
          schema:
            type: string
      responses:
        '204':
          description: No content
"#,
        );

        let operation = &api.operations[0];
        assert_eq!(parameter(operation, "user-id").rust_name, "user_id");
        assert_eq!(parameter(operation, "user_id").rust_name, "user_id_2");
    }

    #[test]
    fn renames_request_body_field_when_parameter_uses_body() {
        let api = parse_valid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /users:
    post:
      operationId: createUser
      parameters:
        - name: body
          in: query
          schema:
            type: string
      requestBody:
        content:
          application/json:
            schema:
              type: string
      responses:
        '204':
          description: No content
"#,
        );

        let operation = &api.operations[0];
        assert_eq!(parameter(operation, "body").rust_name, "body");
        assert_eq!(
            operation
                .request_body
                .as_ref()
                .expect("request body")
                .field_name,
            "body_2"
        );
    }

    #[test]
    fn api_key_names_do_not_collide_with_builder_methods() {
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
  securitySchemes:
    newKey:
      type: apiKey
      in: header
      name: new
    applyKey:
      type: apiKey
      in: header
      name: apply
    baseUrlKey:
      type: apiKey
      in: query
      name: base_url
    httpKey:
      type: apiKey
      in: query
      name: http
"#,
        );

        assert_eq!(api_key_rust_name(&api, "new"), "new_2");
        assert_eq!(api_key_rust_name(&api, "apply"), "apply_2");
        assert_eq!(api_key_rust_name(&api, "base_url"), "base_url_2");
        assert_eq!(api_key_rust_name(&api, "http"), "http_2");
    }

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
            ComponentKind::Enum(variants) => {
                assert_eq!(variants[0].rust_name, "Open");
                assert_eq!(variants[1].rust_name, "Closed");
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
            ComponentKind::Enum(variants) => {
                assert_eq!(variants.len(), 2);
                assert_eq!(variants[0].wire_name, "active");
                assert_eq!(variants[0].rust_name, "Active");
                assert_eq!(variants[1].wire_name, "suspended");
                assert_eq!(variants[1].rust_name, "Suspended");
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
    fn response_name_collision_uses_operation_response_suffix() {
        let api = parse_valid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /psi:
    get:
      operationId: psi
      responses:
        '200':
          description: PSI readings
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/PsiResponse'
components:
  schemas:
    PsiResponse:
      type: object
      required:
        - value
      properties:
        value:
          type: integer
"#,
        );

        component(&api, "PsiResponse");

        assert_eq!(api.operations.len(), 1);
        let operation = &api.operations[0];
        assert_eq!(operation.input_name, "PsiInput");
        assert_eq!(operation.response_name, "PsiOperationResponse");
        assert_eq!(
            operation.responses[0].body,
            Some(TypeRef::Named("PsiResponse".to_owned()))
        );
    }

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
    fn parses_x_satay_parse_as_for_string_schemas() {
        let api = parse_valid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    Arrival:
      type: object
      required:
        - stop
        - latitude
        - visit
        - monitored
        - numericMonitored
        - estimatedArrival
        - frequency
        - ratio
      properties:
        stop:
          type: string
          minLength: 1
          x-satay:
            parse-as: u32
        latitude:
          type: string
          x-satay:
            parse-as: f64
        visit:
          type: string
          x-satay:
            parse-as: u8
        monitored:
          type: string
          x-satay:
            parse-as: bool
        numericMonitored:
          type: integer
          x-satay:
            parse-as: bool
        estimatedArrival:
          type: string
          x-satay:
            parse-as: offset-datetime
        frequency:
          type: string
          minimum: 1
          maximum: 60
          x-satay:
            parse-as: integer-range
        ratio:
          type: string
          format: float
          x-satay:
            parse-as: number-range
"#,
        );

        let arrival = component(&api, "Arrival");
        match &arrival.kind {
            ComponentKind::Struct(fields) => {
                assert_eq!(
                    field(fields, "stop").ty,
                    TypeRef::ParsedString(ParseAs::U32)
                );
                assert_eq!(
                    field(fields, "latitude").ty,
                    TypeRef::ParsedString(ParseAs::F64)
                );
                assert_eq!(
                    field(fields, "visit").ty,
                    TypeRef::ParsedString(ParseAs::U8)
                );
                assert_eq!(
                    field(fields, "monitored").ty,
                    TypeRef::ParsedString(ParseAs::Bool)
                );
                assert_eq!(
                    field(fields, "numericMonitored").ty,
                    TypeRef::ParsedInteger(ParseAs::Bool)
                );
                assert_eq!(
                    field(fields, "estimatedArrival").ty,
                    TypeRef::ParsedString(ParseAs::OffsetDateTime)
                );
                assert_eq!(
                    field(fields, "frequency").ty,
                    TypeRef::Range(RangeTypeRef {
                        rust_name: "ArrivalFrequency".to_owned(),
                        scalar: RangeScalar::Integer(IntegerType::U8),
                    })
                );
                assert_eq!(
                    field(fields, "ratio").ty,
                    TypeRef::Range(RangeTypeRef {
                        rust_name: "ArrivalRatio".to_owned(),
                        scalar: RangeScalar::F32,
                    })
                );
            }
            other => panic!("expected Arrival struct, got {other:?}"),
        }
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
    fn lowers_date_parse_as_on_query_parameters() {
        let api = parse_valid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /psi:
    get:
      operationId: psi
      parameters:
        - name: date
          in: query
          schema:
            type: string
            x-satay:
              parse-as: date
      responses:
        '204':
          description: No content
"#,
        );

        let date = parameter(&api.operations[0], "date");
        assert_eq!(date.ty, TypeRef::ParsedString(ParseAs::Date));
        assert!(!date.required);
    }

    #[test]
    fn lowers_naive_datetime_parse_as_on_query_parameters() {
        let api = parse_valid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /psi:
    get:
      operationId: psi
      parameters:
        - name: date
          in: query
          schema:
            type: string
            x-satay:
              parse-as: naive-datetime
      responses:
        '204':
          description: No content
"#,
        );

        let date = parameter(&api.operations[0], "date");
        assert_eq!(date.ty, TypeRef::ParsedString(ParseAs::NaiveDateTime));
        assert!(!date.required);
    }

    #[test]
    fn rejects_parameters_that_reference_nullable_aliases() {
        let err = parse_invalid(
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
        - name: nickname
          in: query
          schema:
            $ref: '#/components/schemas/OptionalName'
      responses:
        '204':
          description: No content
components:
  schemas:
    OptionalName:
      type: [string, "null"]
"#,
        );

        match err {
            ValidationError::NullableParameterUnsupported { wire_name } => {
                assert_eq!(wire_name, "nickname");
            }
            other => panic!("unexpected error: {other}"),
        }
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
    fn parses_x_satay_enum_variants() {
        let api = parse_valid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    VehicleType:
      type: string
      enum:
        - SD
        - DD
        - BD
        - ""
      x-satay:
        enum-variants:
          SD: SingleDecker
          DD: DoubleDecker
          BD: Bendy
          "": Unknown
    Arrival:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          enum:
            - SD
            - DD
            - BD
            - ""
          x-satay:
            enum-variants:
              SD: SingleDecker
              DD: DoubleDecker
              BD: Bendy
              "": Unknown
"#,
        );

        let vehicle_type = component(&api, "VehicleType");
        match &vehicle_type.kind {
            ComponentKind::Enum(variants) => {
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[0].wire_name, "SD");
                assert_eq!(variants[0].rust_name, "SingleDecker");
                assert_eq!(variants[1].wire_name, "DD");
                assert_eq!(variants[1].rust_name, "DoubleDecker");
                assert_eq!(variants[2].wire_name, "BD");
                assert_eq!(variants[2].rust_name, "Bendy");
            }
            other => panic!("expected VehicleType enum, got {other:?}"),
        }

        let arrival_type = component(&api, "ArrivalType");
        match &arrival_type.kind {
            ComponentKind::Enum(variants) => {
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[0].rust_name, "SingleDecker");
                assert_eq!(variants[1].rust_name, "DoubleDecker");
                assert_eq!(variants[2].rust_name, "Bendy");
            }
            other => panic!("expected ArrivalType enum, got {other:?}"),
        }
    }

    #[test]
    fn rejects_x_satay_enum_variants_for_values_outside_enum() {
        let err = parse_invalid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/VehicleType'
components:
  schemas:
    VehicleType:
      type: string
      enum:
        - SD
      x-satay:
        enum-variants:
          DD: DoubleDecker
"#,
        );

        match err {
            ValidationError::UnknownSatayEnumVariantValue { context, wire_name } => {
                assert_eq!(context, "schema `VehicleType`");
                assert_eq!(wire_name, "DD");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn parses_x_satay_treat_error_as_none() {
        let api = parse_valid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    Arrival:
      type: object
      required:
        - timing
      properties:
        timing:
          type: string
          x-satay:
            treat-error-as-none: true
        optionalTiming:
          type: string
"#,
        );

        let arrival = component(&api, "Arrival");
        match &arrival.kind {
            ComponentKind::Struct(fields) => {
                let timing = field(fields, "timing");
                assert!(timing.treat_error_as_none);
                let optional_timing = field(fields, "optionalTiming");
                assert!(!optional_timing.treat_error_as_none);
            }
            other => panic!("expected Arrival struct, got {other:?}"),
        }
    }

    #[test]
    fn validates_x_satay_parse_as_on_reachable_operation_schemas() {
        let err = parse_invalid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      parameters:
        - name: includeDetails
          in: query
          schema:
            type: boolean
            x-satay:
              parse-as: u8
      responses:
        '204':
          description: No content
"#,
        );

        match err {
            ValidationError::SatayParseAsRequiresString {
                context,
                parse_as,
                kind,
            } => {
                assert_eq!(context, "parameter `includeDetails`");
                assert_eq!(parse_as, "u8");
                assert_eq!(kind, "boolean");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn validates_x_satay_integer_type_on_reachable_request_body_schema() {
        let err = parse_invalid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    post:
      operationId: createArrival
      requestBody:
        content:
          application/json:
            schema:
              type: string
              x-satay:
                integer-type: u8
      responses:
        '204':
          description: No content
"#,
        );

        match err {
            ValidationError::SatayIntegerTypeRequiresInteger {
                context,
                integer_type,
                kind,
            } => {
                assert_eq!(context, "operation `createArrival` requestBody");
                assert_eq!(integer_type, "u8");
                assert_eq!(kind, "string");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn validates_x_satay_treat_error_as_none_on_struct_properties() {
        let err = parse_invalid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    Arrival:
      type: object
      properties:
        timing:
          type: string
          x-satay:
            treat-error-as-none: yes
"#,
        );

        match err {
            ValidationError::InvalidBooleanKeyword { context, keyword } => {
                assert_eq!(context, "property `Arrival.timing`");
                assert_eq!(keyword, "treat-error-as-none");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn skips_x_satay_validation_for_unreachable_component_parameters() {
        parse_valid(
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
  parameters:
    BrokenButUnused:
      name: includeDetails
      in: query
      schema:
        type: boolean
        x-satay:
          parse-as: u8
"#,
        );
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

    #[test]
    fn reports_reference_and_path_validation_errors() {
        let err = parse_invalid(
            r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /users/{userId}:
    get:
      operationId: getUser
      parameters:
        - name: accountId
          in: path
          required: true
          schema:
            type: string
      responses:
        '204':
          description: No content
"#,
        );
        match err {
            ValidationError::UndeclaredPathParameter { path, name } => {
                assert_eq!(path, "/users/{userId}");
                assert_eq!(name, "userId");
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
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
        - $ref: '#/components/parameters/Missing'
      responses:
        '204':
          description: No content
components:
  parameters: {}
"##,
        );
        match err {
            ValidationError::ResolveReference {
                reference,
                context,
                source,
            } => {
                assert_eq!(reference, "#/components/parameters/Missing");
                assert_eq!(context, "operation `getUser` parameters");
                match *source {
                    ValidationError::MissingJsonPointerToken { token } => {
                        assert_eq!(token, "Missing");
                    }
                    other => panic!("unexpected reference source: {other}"),
                }
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn rejects_circular_component_references_during_resolution() {
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
  parameters:
    A:
      $ref: '#/components/parameters/B'
    B:
      $ref: '#/components/parameters/A'
"##,
        );

        match err {
            ValidationError::ResolveReference {
                reference,
                context,
                source,
            } => {
                assert_eq!(reference, "#/components/parameters/B");
                assert_eq!(context, "parameter `A`");
                match *source {
                    ValidationError::CircularReference { reference } => {
                        assert_eq!(reference, "#/components/parameters/B");
                    }
                    other => panic!("unexpected reference source: {other}"),
                }
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn rejects_openapi_30_documents() {
        let err = parse_invalid(
            r#"
openapi: 3.0.3
info:
  title: Test API
  version: 1.0.0
paths: {}
"#,
        );

        match err {
            ValidationError::UnsupportedOpenApiVersion { version } => {
                assert_eq!(version, "3.0.3");
            }
            other => panic!("unexpected error: {other}"),
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
    fn rejects_unsupported_openapi_31_schema_forms_explicitly() {
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
    Broken: true
"#,
        );
        match err {
            ValidationError::UnsupportedBooleanSchema { context } => {
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
      type: [string, integer, "null"]
"#,
        );
        match err {
            ValidationError::MultipleNonNullSchemaTypesUnsupported { context } => {
                assert_eq!(context, "schema `Broken`");
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
