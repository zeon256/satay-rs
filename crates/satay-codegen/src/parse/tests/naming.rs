use super::ast::*;
use super::*;

#[test]
fn deduplicates_parameter_field_names_after_identifier_sanitization() {
    let files = generate_valid(
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

    let api_file = parse_rust(file(&files, "api.rs"));
    assert!(has_method(&api_file, "ListUsersAction", "user_id"));
    assert!(has_method(&api_file, "ListUsersAction", "user_id_2"));

    let parts = parse_rust(file(&files, "list_users/parts.rs"));
    let input = find_struct(&parts, "ListUsersInput");
    assert_eq!(field_names(input), ["user_id", "user_id_2"]);
}

#[test]
fn renames_request_body_field_when_parameter_uses_body() {
    let files = generate_valid(
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

    let api_file = parse_rust(file(&files, "api.rs"));
    assert!(has_method(&api_file, "CreateUserAction", "body"));
    assert!(has_method(&api_file, "CreateUserAction", "body_2"));

    let parts = parse_rust(file(&files, "create_user/parts.rs"));
    let input = find_struct(&parts, "CreateUserInput");
    assert_eq!(field_names(input), ["body", "body_2"]);
}

#[test]
fn api_key_names_do_not_collide_with_builder_methods() {
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
    storageKey:
      type: apiKey
      in: header
      name: string_storage
    httpKey:
      type: apiKey
      in: query
      name: http
"#,
    );

    let api_file = parse_rust(file(&files, "api.rs"));
    for name in [
        "new_2",
        "apply_2",
        "base_url_2",
        "http_2",
        "string_storage_2",
    ] {
        assert!(
            has_method(&api_file, "Api", name),
            "missing API key builder method {name}"
        );
    }
    // The colliding originals remain under their non-colliding identities.
    assert!(has_method(&api_file, "Api", "base_url"));
    assert!(has_method(&api_file, "Api", "string_storage"));
}

#[test]
fn response_name_collision_uses_operation_response_suffix() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    find_struct(&types, "PsiResponse");

    let parts = parse_rust(file(&files, "psi/parts.rs"));
    find_struct(&parts, "PsiInput");

    // The response case keeps the component name; the generated response
    // wrapper carries the operation-response suffix.
    let json = parse_rust(file(&files, "psi/json.rs"));
    assert!(contains_tokens(
        find_fn(&json, "decode_psi_response"),
        "PsiOperationResponse"
    ));
}
