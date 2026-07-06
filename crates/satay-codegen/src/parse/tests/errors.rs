use super::*;

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
fn reports_undeclared_path_parameters() {
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
}

#[test]
fn reports_unresolvable_parameter_references() {
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
fn rejects_boolean_schemas() {
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
}

#[test]
fn rejects_multiple_non_null_schema_types() {
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

#[test]
fn rejects_status_range_above_5xx() {
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
        '6XX':
          description: Bogus range
"#,
    );
    match err {
        ValidationError::InvalidStatusCode { status, .. } => {
            assert_eq!(status, "6XX");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_lowercase_status_range() {
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
        '4xx':
          description: Lowercase wildcard
"#,
    );
    match err {
        ValidationError::InvalidStatusCode { status, .. } => {
            assert_eq!(status, "4xx");
        }
        other => panic!("unexpected error: {other}"),
    }
}
