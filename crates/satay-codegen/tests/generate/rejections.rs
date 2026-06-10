use satay_codegen::{Error, ValidationError};

#[test]
fn openapi_30_documents_are_rejected() {
    let err = satay_codegen::generate(
        r#"
openapi: 3.0.3
info:
  title: OpenAPI 3.0 API
  version: 1.0.0
paths: {}
"#,
    )
    .expect_err("OpenAPI 3.0 documents are unsupported");

    match err {
        Error::Validation(ValidationError::UnsupportedOpenApiVersion { version }) => {
            assert_eq!(version, "3.0.3");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn nullable_parameters_are_rejected_instead_of_generating_invalid_rust() {
    let err = satay_codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Nullable parameter
  version: 1.0.0
paths:
  /users/{userId}:
    get:
      operationId: getUser
      parameters:
        - name: userId
          in: path
          required: true
          schema:
            type: [string, "null"]
      responses:
        '204':
          description: No content
"#,
    )
    .expect_err("nullable parameters are unsupported");

    match err {
        Error::Validation(ValidationError::NullableParameterUnsupported { wire_name, .. }) => {
            assert_eq!(wire_name, "userId");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn default_response_bodies_are_rejected_instead_of_silently_dropped() {
    let err = satay_codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Default response
  version: 1.0.0
paths:
  /ping:
    get:
      operationId: ping
      responses:
        default:
          description: Error response
          content:
            application/json:
              schema:
                type: string
"#,
    )
    .expect_err("default response bodies are unsupported");

    match err {
        Error::Validation(ValidationError::DefaultResponseBodyUnsupported { context, .. }) => {
            assert_eq!(context, "operation `ping` responses");
        }
        other => panic!("unexpected error: {other}"),
    }
}
