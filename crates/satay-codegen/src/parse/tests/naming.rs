use super::*;

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
