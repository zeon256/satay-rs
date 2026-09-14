use super::{definition, normalize, object, string};
use crate::error::ValidationError;
use crate::model::TypeRef;
use crate::parse::normalize::{NormalizeError, normalize_spec};
use crate::parse::tests::parse_valid;
use satay_ir::{
    AdditionalProperties, ApiKeyLocation, CompositionKind, HttpMethod, OAuthFlowKind,
    ParameterLocation, ParameterStyle, ResponseStatus, SchemaUse, SecuritySchemeKind, SourceRef,
    TypeExpr,
};
use serde_json::{Value, json};

fn assert_source(source: &Option<SourceRef>, pointer: &str) {
    assert_eq!(
        source.as_ref(),
        Some(&SourceRef {
            document: "test.yaml".to_owned(),
            pointer: pointer.to_owned(),
        })
    );
}

fn array(value: &SchemaUse) -> &satay_ir::ArraySchema {
    match &value.ty {
        TypeExpr::Array(array) => array,
        other => panic!("expected array, got {other:?}"),
    }
}

fn operation_spec(operation: Value) -> String {
    json!({
        "openapi": "3.1.0",
        "info": {"title": "HTTP", "version": "1"},
        "paths": {"/items": {"get": operation}},
    })
    .to_string()
}

fn validation_error(spec: &str, pointer: &str) -> ValidationError {
    match normalize_spec(spec, "test.yaml").expect_err("invalid HTTP declaration") {
        NormalizeError::Validation { location, source } => {
            assert_eq!(location.document, "test.yaml");
            assert_eq!(location.pointer, pointer);
            *source
        }
        other => panic!("expected located validation error, got {other:?}"),
    }
}

#[test]
fn retains_http_metadata_and_explicit_inheritance_states() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Metadata, version: '1'}
servers:
  - url: 'https://{region}.example.test/{version}'
    description: '  primary  '
    variables:
      version: {default: v2, enum: [v2, v1], description: API version}
      region: {default: west, enum: [west, east], description: Region}
  - url: /fallback
    description: fallback
tags:
  - {name: unused, description: Declared but unused}
  - {name: second, description: Second group}
  - {name: first, description: First group}
security:
  - OAuth: [write, read]
    HeaderKey: []
  - {}
  - UndeclaredScheme: [custom]
paths:
  /zeta:
    servers: []
    trace:
      responses: {'204': {description: trace}}
    options:
      responses: {'204': {description: options}}
    head:
      responses: {'204': {description: head}}
    delete:
      responses: {'204': {description: deleted}}
    patch:
      operationId: patch-declared
      security: [{CookieKey: []}, {}]
      servers: [{url: /operation, description: local}]
      responses: {'204': {description: updated}}
    post:
      security: []
      servers: []
      responses: {'204': {description: created}}
    put:
      security: [{}]
      responses: {'204': {description: replaced}}
    get:
      operationId: declared id unchanged
      description: '  operation description  '
      tags: [second, first, second]
      responses: {'204': {description: loaded}}
  /alpha:
    servers:
      - url: '/path/{version}'
        variables:
          version: {default: stable, enum: [stable, next]}
    get:
      responses: {'204': {description: ok}}
  /absent:
    get:
      responses: {'204': {description: ok}}
components:
  securitySchemes:
    HeaderKey: {type: apiKey, in: header, name: X-Exact-Key, description: Header key}
    QueryKey: {type: apiKey, in: query, name: api_key}
    CookieKey: {type: apiKey, in: cookie, name: session}
    Bearer: {type: http, scheme: Bearer, bearerFormat: JWT, description: Bearer token}
    OAuth:
      type: oauth2
      description: All flows
      flows:
        authorizationCode:
          authorizationUrl: https://auth.example.test/code
          tokenUrl: https://auth.example.test/code-token
          refreshUrl: https://auth.example.test/code-refresh
          scopes: {code: Code grant}
        clientCredentials:
          tokenUrl: https://auth.example.test/client-token
          refreshUrl: https://auth.example.test/client-refresh
          scopes: {client: Client grant}
        password:
          tokenUrl: https://auth.example.test/password-token
          refreshUrl: https://auth.example.test/password-refresh
          scopes: {password: Password grant}
        implicit:
          authorizationUrl: https://AUTH.example.test:443
          refreshUrl: https://auth.example.test/implicit-refresh
          scopes: {write: Write things, read: Read things}
    Discovery:
      type: openIdConnect
      openIdConnectUrl: https://identity.example.test/.well-known/openid-configuration
      description: Discovery document
    Certificate: {type: mutualTLS, description: Client certificate}
    Alias: {$ref: '#/components/securitySchemes/Bearer', description: ignored override}
"#,
    );
    let http = api.http();
    assert_eq!(
        http.paths
            .iter()
            .map(|path| path.path.as_str())
            .collect::<Vec<_>>(),
        ["/zeta", "/alpha", "/absent"]
    );

    let operations = &http.paths[0].operations;
    assert_eq!(
        operations
            .iter()
            .map(|operation| operation.method)
            .collect::<Vec<_>>(),
        [
            HttpMethod::Get,
            HttpMethod::Post,
            HttpMethod::Put,
            HttpMethod::Patch,
            HttpMethod::Delete,
            HttpMethod::Head,
            HttpMethod::Options,
            HttpMethod::Trace,
        ]
    );

    assert_eq!(
        operations[0].source_id.as_deref(),
        Some("declared id unchanged")
    );
    assert_eq!(operations[1].source_id, None);
    assert_eq!(
        operations[0].description.as_deref(),
        Some("  operation description  ")
    );
    assert_eq!(operations[0].tags, ["second", "first", "second"]);
    assert!(
        operations
            .iter()
            .all(|operation| !operation.interpretation.skip)
    );

    assert_source(&operations[0].source, "/paths/~1zeta/get");
    assert_eq!(http.paths[0].servers, Some(vec![]));
    assert_eq!(http.paths[2].servers, None);
    assert_eq!(operations[0].servers, None);
    assert_eq!(operations[1].servers, Some(vec![]));
    assert_eq!(operations[3].servers.as_ref().unwrap()[0].url, "/operation");
    assert_eq!(
        operations[3].servers.as_ref().unwrap()[0]
            .description
            .as_deref(),
        Some("local")
    );

    let path_server = &http.paths[1].servers.as_ref().unwrap()[0];
    assert_eq!(path_server.url, "/path/{version}");
    assert_eq!(path_server.variables[0].enum_values, ["stable", "next"]);
    assert_eq!(
        http.servers[0].url,
        "https://{region}.example.test/{version}"
    );

    assert_eq!(http.servers[0].description.as_deref(), Some("  primary  "));
    assert_eq!(
        http.servers[0]
            .variables
            .iter()
            .map(|variable| variable.name.as_str())
            .collect::<Vec<_>>(),
        ["version", "region"]
    );
    assert_eq!(http.servers[0].variables[0].default, "v2");
    assert_eq!(http.servers[0].variables[0].enum_values, ["v2", "v1"]);
    assert_eq!(
        http.servers[0].variables[1].description.as_deref(),
        Some("Region")
    );
    assert_eq!(http.servers[1].url, "/fallback");
    assert_eq!(
        http.tags
            .iter()
            .map(|tag| (tag.name.as_str(), tag.description.as_deref()))
            .collect::<Vec<_>>(),
        [
            ("unused", Some("Declared but unused")),
            ("second", Some("Second group")),
            ("first", Some("First group"))
        ]
    );

    assert_eq!(operations[0].security, None);
    assert_eq!(operations[1].security, Some(vec![]));
    assert!(
        operations[2].security.as_ref().unwrap()[0]
            .schemes
            .is_empty()
    );

    let local = operations[3].security.as_ref().unwrap();
    assert_eq!(local[0].schemes[0].scheme, "CookieKey");
    assert!(local[1].schemes.is_empty());
    assert_eq!(
        http.security[0]
            .schemes
            .iter()
            .map(|scheme| scheme.scheme.as_str())
            .collect::<Vec<_>>(),
        ["OAuth", "HeaderKey"]
    );

    assert_eq!(http.security[0].schemes[0].scopes, ["write", "read"]);
    assert!(http.security[1].schemes.is_empty());
    assert_eq!(http.security[2].schemes[0].scheme, "UndeclaredScheme");
    assert_eq!(http.security[2].schemes[0].scopes, ["custom"]);

    let schemes = &http.security_schemes;

    assert_eq!(
        schemes
            .iter()
            .map(|scheme| scheme.name.as_str())
            .collect::<Vec<_>>(),
        [
            "HeaderKey",
            "QueryKey",
            "CookieKey",
            "Bearer",
            "OAuth",
            "Discovery",
            "Certificate",
            "Alias"
        ]
    );

    for (scheme, wire_name, location) in [
        (&schemes[0], "X-Exact-Key", ApiKeyLocation::Header),
        (&schemes[1], "api_key", ApiKeyLocation::Query),
        (&schemes[2], "session", ApiKeyLocation::Cookie),
    ] {
        assert_eq!(
            scheme.kind,
            SecuritySchemeKind::ApiKey {
                wire_name: wire_name.to_owned(),
                location
            }
        );
    }
    assert_eq!(schemes[0].description.as_deref(), Some("Header key"));
    assert_eq!(
        schemes[3].kind,
        SecuritySchemeKind::Http {
            scheme: "Bearer".to_owned(),
            bearer_format: Some("JWT".to_owned()),
        }
    );
    assert_eq!(schemes[7].kind, schemes[3].kind);
    assert_eq!(schemes[7].description.as_deref(), Some("Bearer token"));
    assert_eq!(
        schemes[5].kind,
        SecuritySchemeKind::OpenIdConnect {
            url: "https://identity.example.test/.well-known/openid-configuration".to_owned(),
        }
    );
    assert_eq!(schemes[6].kind, SecuritySchemeKind::MutualTls);
    assert_eq!(
        schemes[6].description.as_deref(),
        Some("Client certificate")
    );
    let SecuritySchemeKind::OAuth2 { flows } = &schemes[4].kind else {
        panic!("OAuth2 scheme");
    };
    assert_eq!(
        flows.iter().map(|flow| flow.kind).collect::<Vec<_>>(),
        [
            OAuthFlowKind::Implicit,
            OAuthFlowKind::Password,
            OAuthFlowKind::ClientCredentials,
            OAuthFlowKind::AuthorizationCode,
        ]
    );
    assert_eq!(
        flows[0].authorization_url.as_deref(),
        Some("https://auth.example.test/")
    );
    assert_eq!(flows[0].token_url, None);
    assert_eq!(
        flows[0].refresh_url.as_deref(),
        Some("https://auth.example.test/implicit-refresh")
    );
    assert_eq!(
        flows[0]
            .scopes
            .iter()
            .map(|scope| (scope.name.as_str(), scope.description.as_str()))
            .collect::<Vec<_>>(),
        [("write", "Write things"), ("read", "Read things"),]
    );
    assert_eq!(flows[1].authorization_url, None);
    assert_eq!(
        flows[1].token_url.as_deref(),
        Some("https://auth.example.test/password-token")
    );
    assert_eq!(
        flows[1].refresh_url.as_deref(),
        Some("https://auth.example.test/password-refresh")
    );
    assert_eq!(flows[1].scopes[0].name, "password");
    assert_eq!(flows[2].authorization_url, None);
    assert_eq!(
        flows[2].token_url.as_deref(),
        Some("https://auth.example.test/client-token")
    );
    assert_eq!(
        flows[2].refresh_url.as_deref(),
        Some("https://auth.example.test/client-refresh")
    );
    assert_eq!(flows[2].scopes[0].name, "client");
    assert_eq!(
        flows[3].authorization_url.as_deref(),
        Some("https://auth.example.test/code")
    );
    assert_eq!(
        flows[3].token_url.as_deref(),
        Some("https://auth.example.test/code-token")
    );
    assert_eq!(
        flows[3].refresh_url.as_deref(),
        Some("https://auth.example.test/code-refresh")
    );
    assert_eq!(flows[3].scopes[0].description, "Code grant");
}

#[test]
fn follows_terminal_http_component_origins_without_fabricated_children() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Origins, version: '1'}
paths:
  '/~things/{id}':
    $ref: '#/components/pathItems/PathAlias'
    servers: [{url: /ignored-sibling}]
components:
  schemas:
    DefaultText: {type: string, default: inherited-only-later}
  pathItems:
    PathAlias: {$ref: '#/components/pathItems/P~1~0'}
    'P/~':
      servers: []
      parameters:
        - {$ref: '#/components/parameters/ParameterAlias'}
        - name: defaulted
          in: query
          schema: {$ref: '#/components/schemas/DefaultText'}
      get:
        operationId: read
        servers: []
        security: []
        parameters:
          - name: X-Values
            in: header
            style: simple
            explode: false
            schema: {type: array, items: {type: string}}
          - name: defaulted
            in: query
            schema: {type: string, default: local}
        requestBody: {$ref: '#/components/requestBodies/BodyAlias', description: ignored body override}
        responses:
          '200': {$ref: '#/components/responses/ResponseAlias', description: ignored response override}
  parameters:
    ParameterAlias: {$ref: '#/components/parameters/ParameterHop'}
    ParameterHop: {$ref: '#/components/parameters/id~1~0', description: ignored parameter override}
    'id/~':
      name: id
      in: path
      required: true
      description: Actual parameter
      style: label
      explode: false
      allowReserved: true
      allowEmptyValue: false
      schema: {type: [string, 'null'], default: null}
  requestBodies:
    BodyAlias: {$ref: '#/components/requestBodies/BodyHop'}
    BodyHop: {$ref: '#/components/requestBodies/B~1~0'}
    'B/~':
      description: Actual body
      required: true
      content:
        'text/x-~wire':
          schema:
            type: object
            properties:
              'field/~': {type: string, default: null}
        application/json:
          schema: {type: [string, 'null'], default: null}
        application/octet-stream: {}
  responses:
    ResponseAlias: {$ref: '#/components/responses/ResponseHop'}
    ResponseHop: {$ref: '#/components/responses/R~1~0'}
    'R/~':
      description: Actual response
      content:
        'Application/Vnd.Row+JSON; profile=x~y':
          schema: {type: object, properties: {extra: {type: integer}}}
        application/json:
          schema: {type: [string, 'null'], default: null}
        image/png: {}
"#,
    );

    let path = &api.http().paths[0];
    assert_eq!(path.path, "/~things/{id}");
    assert_source(&path.source, "/paths/~1~0things~1{id}");
    assert_eq!(path.servers, Some(vec![]));
    assert_eq!(
        path.parameters
            .iter()
            .map(|parameter| parameter.wire_name.as_str())
            .collect::<Vec<_>>(),
        ["id", "defaulted"]
    );

    let parameter = &path.parameters[0];
    assert_eq!(parameter.description.as_deref(), Some("Actual parameter"));
    assert_eq!(parameter.location, ParameterLocation::Path);
    assert!(parameter.required);
    assert_eq!(parameter.style, Some(ParameterStyle::Label));
    assert_eq!(parameter.explode, Some(false));
    assert_eq!(parameter.allow_reserved, Some(true));
    assert_eq!(parameter.allow_empty_value, Some(false));
    assert_source(
        &parameter.source,
        "/components/pathItems/P~1~0/parameters/0",
    );
    assert_source(
        &parameter.schema.annotations.source,
        "/components/parameters/id~1~0/schema",
    );

    assert_eq!(parameter.schema.annotations.default, Some(Value::Null));
    assert!(parameter.schema.nullable);

    let defaulted = &path.parameters[1];
    assert_eq!(defaulted.schema.annotations.default, None);
    assert_eq!(defaulted.style, None);
    assert_eq!(defaulted.explode, None);
    assert_eq!(defaulted.allow_reserved, None);
    assert_eq!(defaulted.allow_empty_value, None);
    assert_source(
        &defaulted.schema.annotations.source,
        "/components/pathItems/P~1~0/parameters/1/schema",
    );
    let TypeExpr::Ref(id) = defaulted.schema.ty else {
        panic!("defaulted schema reference")
    };
    assert_eq!(
        api.definition(id).unwrap().schema.annotations.default,
        Some(json!("inherited-only-later"))
    );

    let operation = &path.operations[0];
    assert_source(&operation.source, "/components/pathItems/P~1~0/get");
    assert_eq!(operation.security, Some(vec![]));
    assert_eq!(operation.servers, Some(vec![]));
    assert_eq!(
        operation
            .parameters
            .iter()
            .map(|parameter| parameter.wire_name.as_str())
            .collect::<Vec<_>>(),
        ["X-Values", "defaulted"]
    );
    assert_source(
        &operation.parameters[0].source,
        "/components/pathItems/P~1~0/get/parameters/0",
    );
    assert_eq!(operation.parameters[0].location, ParameterLocation::Header);
    assert!(!operation.parameters[0].required);
    assert_eq!(operation.parameters[0].style, Some(ParameterStyle::Simple));
    assert_eq!(operation.parameters[0].explode, Some(false));
    assert!(matches!(
        array(&operation.parameters[0].schema).items.ty,
        TypeExpr::String(_)
    ));
    assert_eq!(
        operation.parameters[1].schema.annotations.default,
        Some(json!("local"))
    );
    assert_eq!(path.parameters[1].schema.annotations.default, None);

    let body = operation.request_body.as_ref().unwrap();
    assert!(body.required);
    assert_eq!(body.description.as_deref(), Some("Actual body"));
    assert_source(&body.source, "/components/pathItems/P~1~0/get/requestBody");
    assert_eq!(
        body.content
            .iter()
            .map(|media| media.media_type.as_str())
            .collect::<Vec<_>>(),
        [
            "text/x-~wire",
            "application/json",
            "application/octet-stream"
        ]
    );
    assert_source(
        &body.content[0].source,
        "/components/requestBodies/B~1~0/content/text~1x-~0wire",
    );
    let alternative = body.content[0].schema.as_ref().unwrap();
    assert_source(
        &object(alternative).properties[0].value.annotations.source,
        "/components/requestBodies/B~1~0/content/text~1x-~0wire/schema/properties/field~1~0",
    );
    assert_eq!(
        object(alternative).properties[0].value.annotations.default,
        Some(Value::Null)
    );
    let request_schema = body.content[1].schema.as_ref().unwrap();
    assert_source(
        &request_schema.annotations.source,
        "/components/requestBodies/B~1~0/content/application~1json/schema",
    );
    assert_eq!(request_schema.annotations.default, Some(Value::Null));
    assert!(body.content[2].schema.is_none());
    assert_source(
        &body.content[2].source,
        "/components/requestBodies/B~1~0/content/application~1octet-stream",
    );

    let response = &operation.responses[0];
    assert_eq!(response.description.as_deref(), Some("Actual response"));
    assert_source(
        &response.source,
        "/components/pathItems/P~1~0/get/responses/200",
    );
    assert_eq!(
        response
            .content
            .iter()
            .map(|media| media.media.media_type.as_str())
            .collect::<Vec<_>>(),
        [
            "Application/Vnd.Row+JSON; profile=x~y",
            "application/json",
            "image/png"
        ]
    );
    assert_source(
        &response.content[0].media.source,
        "/components/responses/R~1~0/content/Application~1Vnd.Row+JSON; profile=x~0y",
    );
    let response_schema = response.content[1].media.schema.as_ref().unwrap();
    assert_source(
        &response_schema.annotations.source,
        "/components/responses/R~1~0/content/application~1json/schema",
    );
    assert_eq!(response_schema.annotations.default, Some(Value::Null));
    assert!(response_schema.nullable);
    assert!(response.content[2].media.schema.is_none());
    assert_source(
        &response.content[2].media.source,
        "/components/responses/R~1~0/content/image~1png",
    );
}

#[test]
fn retains_response_order_and_projects_only_selected_json_without_folding_absence() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Responses, version: '1'}
paths:
  /items:
    get:
      x-satay: {output: {unwrap-field: value}}
      responses:
        default: {description: fallback}
        '204': {description: empty}
        '2XX':
          description: family
          content:
            application/vendor+json:
              schema: {type: object, properties: {other: {type: boolean}}}
            application/json:
              schema:
                type: object
                required: [extra]
                properties:
                  value: {type: string, description: projected}
                  extra: {type: integer}
            text/plain: {}
        '200':
          description: exact
          content:
            'Application/Vnd.Exact+JSON; charset=utf-8':
              schema:
                type: object
                properties:
                  value: {type: [string, 'null']}
        '400':
          description: error body is projected too
          content:
            application/json:
              schema: {type: object, properties: {value: {type: boolean}}}
"#,
    );

    let operation = &api.http().paths[0].operations[0];
    assert_eq!(operation.source_id, None);
    assert_eq!(
        operation
            .interpretation
            .output
            .as_ref()
            .unwrap()
            .unwrap_field,
        "value"
    );

    let responses = &operation.responses;
    assert_eq!(
        responses
            .iter()
            .map(|response| response.status)
            .collect::<Vec<_>>(),
        [
            ResponseStatus::Default,
            ResponseStatus::Exact(204),
            ResponseStatus::Range(2),
            ResponseStatus::Exact(200),
            ResponseStatus::Exact(400),
        ]
    );
    assert!(responses[0].content.is_empty());
    assert_eq!(responses[0].description.as_deref(), Some("fallback"));
    assert_source(&responses[0].source, "/paths/~1items/get/responses/default");
    assert!(responses[1].content.is_empty());
    let family = &responses[2].content;
    assert_eq!(
        family
            .iter()
            .map(|media| media.media.media_type.as_str())
            .collect::<Vec<_>>(),
        ["application/vendor+json", "application/json", "text/plain"]
    );
    assert!(family[0].projection.is_none());
    assert_eq!(
        object(family[0].media.schema.as_ref().unwrap()).properties[0].wire_name,
        "other"
    );
    assert!(family[2].media.schema.is_none());
    let original = object(family[1].media.schema.as_ref().unwrap());
    assert_eq!(
        original
            .properties
            .iter()
            .map(|property| (property.wire_name.as_str(), property.required))
            .collect::<Vec<_>>(),
        [("value", false), ("extra", true)]
    );
    let projection = family[1].projection.as_ref().unwrap();
    assert_eq!(
        projection.selector,
        *operation.interpretation.output.as_ref().unwrap()
    );
    assert!(!projection.output.nullable);
    assert!(matches!(projection.output.ty, TypeExpr::String(_)));
    assert_eq!(
        projection.output.annotations.description.as_deref(),
        Some("projected")
    );
    assert_source(
        &projection.output.annotations.source,
        "/paths/~1items/get/responses/2XX/content/application~1json/schema/properties/value",
    );
    assert!(
        responses[3].content[0]
            .projection
            .as_ref()
            .unwrap()
            .output
            .nullable
    );
    assert_eq!(
        responses[3].content[0].media.media_type,
        "Application/Vnd.Exact+JSON; charset=utf-8"
    );
    assert!(matches!(
        responses[4].content[0]
            .projection
            .as_ref()
            .unwrap()
            .output
            .ty,
        TypeExpr::Boolean
    ));
}

#[test]
fn map_projection_retains_inline_envelope_and_declared_array_nullability() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Map output, version: '1'}
paths:
  /items:
    get:
      x-satay: {output: {unwrap-field: 'values/~', map-field: 'link/~'}}
      responses:
        '200':
          description: rows
          content:
            application/json:
              schema:
                type: object
                properties:
                  ignored-by-selector: {type: boolean}
                  'values/~':
                    type: [array, 'null']
                    description: Rows on wire
                    format: declared-array-format
                    default: null
                    minItems: 1
                    maxItems: 3
                    items:
                      type: object
                      required: [extra]
                      properties:
                        'link/~': {type: string, minLength: 2}
                        extra: {type: integer}
"#,
    );
    let media = &api.http().paths[0].operations[0].responses[0].content[0];
    let envelope = object(media.media.schema.as_ref().unwrap());
    assert_eq!(envelope.properties[0].wire_name, "ignored-by-selector");
    let values = &envelope.properties[1];
    assert!(!values.required);
    let row = object(&array(&values.value).items);
    assert_eq!(
        row.properties
            .iter()
            .map(|property| (property.wire_name.as_str(), property.required))
            .collect::<Vec<_>>(),
        [("link/~", false), ("extra", true)]
    );
    let projection = media.projection.as_ref().unwrap();
    assert_eq!(projection.selector.unwrap_field, "values/~");
    assert_eq!(projection.selector.map_field.as_deref(), Some("link/~"));
    assert!(projection.output.nullable);
    assert_eq!(projection.output.annotations.default, Some(Value::Null));
    assert_eq!(
        projection.output.annotations.description.as_deref(),
        Some("Rows on wire")
    );
    assert_eq!(
        projection.output.annotations.format.as_deref(),
        Some("declared-array-format")
    );
    let projected = array(&projection.output);
    assert_eq!(projected.constraints.min_items, Some(1));
    assert_eq!(projected.constraints.max_items, Some(3));
    assert!(!projected.items.nullable);
    assert_eq!(string(&projected.items).constraints.min_length, Some(2));
    assert_source(
        &projection.output.annotations.source,
        "/paths/~1items/get/responses/200/content/application~1json/schema/properties/values~1~0",
    );
    assert_source(
        &projected.items.annotations.source,
        "/paths/~1items/get/responses/200/content/application~1json/schema/properties/values~1~0/items/properties/link~1~0",
    );
}

#[test]
fn projected_schema_aliases_preserve_identity_and_terminal_property_origins() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Referenced output, version: '1'}
paths:
  /items:
    get:
      x-satay: {output: {unwrap-field: values, map-field: 'label/~'}}
      responses:
        '200': {$ref: '#/components/responses/Alias'}
components:
  responses:
    Alias: {$ref: '#/components/responses/Actual'}
    Actual:
      description: rows
      content:
        application/json:
          schema: {$ref: '#/components/schemas/EnvelopeAlias'}
  schemas:
    EnvelopeAlias: {$ref: '#/components/schemas/Envelope'}
    Envelope:
      type: object
      properties:
        values: {$ref: '#/components/schemas/RowsAlias'}
        extra: {type: boolean}
    RowsAlias: {$ref: '#/components/schemas/Rows'}
    Rows:
      type: [array, 'null']
      default: null
      minItems: 2
      items: {$ref: '#/components/schemas/ItemAlias'}
    ItemAlias: {$ref: '#/components/schemas/Item'}
    Item:
      type: object
      properties:
        'label/~': {type: [string, 'null'], default: null}
        extra: {type: boolean}
"#,
    );
    let response = &api.http().paths[0].operations[0].responses[0];
    let media = &response.content[0];
    assert_source(&response.source, "/paths/~1items/get/responses/200");
    assert_source(
        &media.media.source,
        "/components/responses/Actual/content/application~1json",
    );
    let original = media.media.schema.as_ref().unwrap();
    assert_source(
        &original.annotations.source,
        "/components/responses/Actual/content/application~1json/schema",
    );
    let TypeExpr::Ref(id) = original.ty else {
        panic!("original envelope remains reference")
    };
    assert_eq!(api.definition(id).unwrap().source_name, "EnvelopeAlias");
    assert!(matches!(
        api.definition(id).unwrap().schema.ty,
        TypeExpr::Ref(_)
    ));
    assert_eq!(
        object(&definition(&api, "Envelope").schema).properties[1].wire_name,
        "extra"
    );
    let projection = media.projection.as_ref().unwrap();
    assert!(projection.output.nullable);
    assert_eq!(projection.output.annotations.default, Some(Value::Null));
    assert_source(
        &projection.output.annotations.source,
        "/components/schemas/Rows",
    );
    let projected = array(&projection.output);
    assert_eq!(projected.constraints.min_items, Some(2));
    assert!(projected.items.nullable);
    assert_eq!(projected.items.annotations.default, Some(Value::Null));
    assert_source(
        &projected.items.annotations.source,
        "/components/schemas/Item/properties/label~1~0",
    );
}

#[test]
fn preserves_parameter_wire_compositions_and_deferred_encoding_declarations() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Parameters, version: '1'}
paths:
  /items/{ids}:
    parameters:
      - name: ids
        in: path
        required: true
        style: matrix
        explode: true
        schema: {type: array, items: {type: integer}}
    get:
      parameters:
        - name: union
          in: query
          required: true
          allowEmptyValue: true
          allowReserved: false
          schema:
            default: null
            anyOf: [{type: integer}, {type: 'null'}, {type: string}]
        - name: filter
          in: query
          style: deepObject
          explode: true
          schema: {type: object, additionalProperties: {type: string}}
      responses: {'204': {description: ok}}
"#,
    );
    let path = &api.http().paths[0];
    assert_eq!(path.parameters[0].style, Some(ParameterStyle::Matrix));
    assert_eq!(path.parameters[0].explode, Some(true));
    assert!(matches!(
        array(&path.parameters[0].schema).items.ty,
        TypeExpr::Integer(_)
    ));
    let parameters = &path.operations[0].parameters;
    assert!(parameters[0].required);
    assert_eq!(parameters[0].allow_empty_value, Some(true));
    assert_eq!(parameters[0].allow_reserved, Some(false));
    assert_eq!(parameters[0].schema.annotations.default, Some(Value::Null));
    let TypeExpr::Composition(composition) = &parameters[0].schema.ty else {
        panic!("wire union")
    };
    assert_eq!(composition.kind, CompositionKind::AnyOf);
    assert!(matches!(composition.branches[0].ty, TypeExpr::Integer(_)));
    assert!(matches!(composition.branches[1].ty, TypeExpr::Null));
    assert!(matches!(composition.branches[2].ty, TypeExpr::String(_)));
    assert!(!parameters[0].schema.nullable);
    assert_eq!(parameters[1].style, Some(ParameterStyle::DeepObject));
    assert_eq!(parameters[1].explode, Some(true));
    assert!(matches!(
        object(&parameters[1].schema).additional_properties,
        AdditionalProperties::Schema(_)
    ));
}

#[test]
fn missing_paths_differs_from_an_explicit_empty_http_surface() {
    let missing = r#"{"openapi":"3.1.0","info":{"title":"HTTP","version":"1"}}"#;
    assert!(matches!(
        validation_error(missing, ""),
        ValidationError::MissingPaths
    ));
    let empty = r#"{"openapi":"3.1.0","info":{"title":"HTTP","version":"1"},"paths":{}}"#;
    let api = normalize(empty);
    assert!(api.http().paths.is_empty());
    assert!(api.http().servers.is_empty());
    assert!(api.http().security.is_empty());
    assert!(api.http().tags.is_empty());
}

#[test]
fn rejects_unsupported_parameter_declarations_with_structured_payloads() {
    let content = operation_spec(json!({
        "parameters": [{"name":"q","in":"query","content":{"application/json":{"schema":{"type":"string"}}}}],
        "responses": {},
    }));
    assert!(matches!(
        validation_error(&content, "/paths/~1items/get/parameters/0"),
        ValidationError::ContentParameterUnsupported { wire_name, .. } if wire_name == "q"
    ));
    let missing = operation_spec(json!({
        "parameters": [{"name":"q","in":"query"}], "responses": {},
    }));
    assert!(matches!(
        validation_error(&missing, "/paths/~1items/get/parameters/0"),
        ValidationError::MissingParameterSchema { wire_name, .. } if wire_name == "q"
    ));
    let cookie = operation_spec(json!({
        "parameters": [{"name":"session","in":"cookie","schema":{"type":"string"}}],
        "responses": {},
    }));
    assert!(matches!(
        validation_error(&cookie, "/paths/~1items/get/parameters/0"),
        ValidationError::UnsupportedParameterLocation { wire_name, location, .. }
            if wire_name == "session" && location == "cookie"
    ));
    let optional_path = operation_spec(json!({
        "parameters": [{"name":"id","in":"path","required":false,"schema":{"type":"string"}}],
        "responses": {},
    }));
    assert!(matches!(
        validation_error(&optional_path, "/paths/~1items/get/parameters/0"),
        ValidationError::PathParameterNotRequired { wire_name } if wire_name == "id"
    ));
}

#[test]
fn checks_path_placeholders_against_both_parameter_lists() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Template, version: '1'}
paths:
  /{shared}/{local}:
    parameters:
      - {name: shared, in: path, required: true, schema: {type: string}}
    get:
      parameters:
        - {name: local, in: path, required: true, schema: {type: string}}
      responses: {}
"#,
    );
    let path = &api.http().paths[0];
    assert_eq!(path.parameters[0].wire_name, "shared");
    assert_eq!(path.operations[0].parameters[0].wire_name, "local");

    for (path, parameter) in [
        ("/{missing}", None),
        ("/{unclosed", None),
        ("/{}", None),
        (
            "/unused",
            Some(json!({"name":"extra","in":"path","required":true,"schema":{"type":"string"}})),
        ),
    ] {
        let spec = json!({
            "openapi":"3.1.0", "info":{"title":"Template","version":"1"},
            "paths": {(path): {"get": {
                "parameters": parameter.into_iter().collect::<Vec<_>>(),
                "responses": {},
            }}},
        })
        .to_string();
        let pointer = format!("/paths/{}", path.replace('~', "~0").replace('/', "~1"));
        let error = validation_error(&spec, &pointer);
        match path {
            "/{missing}" => assert!(
                matches!(error, ValidationError::UndeclaredPathParameter { name, .. } if name == "missing")
            ),
            "/{unclosed" => assert!(matches!(
                error,
                ValidationError::UnclosedPathParameter { .. }
            )),
            "/{}" => assert!(
                matches!(error, ValidationError::UndeclaredPathParameter { name, .. } if name.is_empty())
            ),
            "/unused" => assert!(
                matches!(error, ValidationError::UnusedPathParameter { name, .. } if name == "extra")
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn enforces_request_json_eligibility_without_selecting_a_more_convenient_schema() {
    let empty = operation_spec(json!({"requestBody":{"content":{}},"responses":{}}));
    assert!(matches!(
        validation_error(&empty, "/paths/~1items/get/requestBody"),
        ValidationError::MissingContent { .. }
    ));
    let non_json = operation_spec(json!({
        "requestBody":{"content":{"text/plain":{"schema":{"type":"string"}}}},
        "responses":{},
    }));
    assert!(matches!(
        validation_error(&non_json, "/paths/~1items/get/requestBody"),
        ValidationError::MissingJsonContent { .. }
    ));
    let exact_without_schema = operation_spec(json!({
        "requestBody":{"content":{
            "application/vendor+json":{"schema":{"type":"string"}},
            "application/json":{},
        }},
        "responses":{},
    }));
    assert!(matches!(
        validation_error(
            &exact_without_schema,
            "/paths/~1items/get/requestBody/content/application~1json"
        ),
        ValidationError::MissingJsonSchema { .. }
    ));
}

#[test]
fn first_recognized_json_selection_keeps_original_media_spelling_and_alternatives() {
    let api = normalize(
        r#"
openapi: 3.1.0
info: {title: Media, version: '1'}
paths:
  /items:
    post:
      requestBody:
        content:
          text/plain: {}
          'Application/Vnd.First+JSON; charset=UTF-8':
            schema: {type: string}
          application/vendor+json:
            schema: {type: object, properties: {extra: {type: integer}}}
      responses:
        '200':
          description: schema-less selected JSON
          content:
            'Application/JSON; charset=UTF-8': {}
            application/vendor+json:
              schema: {type: object, properties: {extra: {type: boolean}}}
"#,
    );
    let operation = &api.http().paths[0].operations[0];
    let request = operation.request_body.as_ref().unwrap();
    assert!(!request.required);
    assert_eq!(
        request
            .content
            .iter()
            .map(|entry| entry.media_type.as_str())
            .collect::<Vec<_>>(),
        [
            "text/plain",
            "Application/Vnd.First+JSON; charset=UTF-8",
            "application/vendor+json",
        ]
    );
    assert!(request.content[0].schema.is_none());
    assert!(matches!(
        request.content[1].schema.as_ref().unwrap().ty,
        TypeExpr::String(_)
    ));
    assert_eq!(
        object(request.content[2].schema.as_ref().unwrap()).properties[0].wire_name,
        "extra"
    );
    let response = &operation.responses[0].content;
    assert_eq!(
        response[0].media.media_type,
        "Application/JSON; charset=UTF-8"
    );
    assert!(response[0].media.schema.is_none());
    assert_eq!(
        object(response[1].media.schema.as_ref().unwrap()).properties[0].wire_name,
        "extra"
    );
    assert!(response.iter().all(|entry| entry.projection.is_none()));
}

#[test]
fn selected_inline_records_remain_outside_value_positions() {
    let request = operation_spec(json!({
        "requestBody":{"content":{"application/json":{"schema":{
            "type":"object","properties":{"name":{"type":"string"}},
        }}}},
        "responses":{},
    }));
    assert!(matches!(
        validation_error(
            &request,
            "/paths/~1items/get/requestBody/content/application~1json/schema"
        ),
        ValidationError::InlineObjectSchema { .. }
    ));
    let response = operation_spec(json!({
        "responses":{"200":{"description":"record","content":{"application/json":{"schema":{
            "type":"object","properties":{"name":{"type":"string"}},
        }}}}},
    }));
    assert!(matches!(
        validation_error(
            &response,
            "/paths/~1items/get/responses/200/content/application~1json/schema"
        ),
        ValidationError::InlineObjectSchema { .. }
    ));
}

#[test]
fn unsupported_alternative_media_fails_explicitly_while_legacy_still_selects_json() {
    let spec = r#"
openapi: 3.1.0
info: {title: All media, version: '1'}
paths:
  /items:
    get:
      operationId: read
      responses:
        '200':
          description: all media
          content:
            application/json:
              schema: {type: string}
            application/xml:
              schema: {type: string, unevaluatedProperties: false}
"#;
    assert!(matches!(
        validation_error(spec, "/paths/~1items/get/responses/200/content/application~1xml/schema/unevaluatedProperties"),
        ValidationError::UnsupportedKeyword { keyword, .. } if keyword == "unevaluatedProperties"
    ));

    let legacy = parse_valid(spec);
    assert_eq!(
        legacy.operations[0].responses[0].body,
        Some(TypeRef::String)
    );

    let request = operation_spec(json!({
        "requestBody":{"content":{
            "application/json":{"schema":{"type":"string"}},
            "image/png":{"schema":false},
        }},
        "responses":{},
    }));

    assert!(matches!(
        validation_error(
            &request,
            "/paths/~1items/get/requestBody/content/image~1png/schema"
        ),
        ValidationError::UnsupportedBooleanSchema { .. }
    ));
}

#[test]
fn rejects_response_structure_statuses_and_default_bodies() {
    let missing = operation_spec(json!({}));
    assert!(matches!(
        validation_error(&missing, "/paths/~1items/get"),
        ValidationError::MissingOperationResponses { operation_id } if operation_id == "get /items"
    ));

    let default_body = operation_spec(json!({
        "responses":{"default":{"description":"fallback","content":{"application/json":{}}}},
    }));
    assert!(matches!(
        validation_error(&default_body, "/paths/~1items/get/responses/default"),
        ValidationError::DefaultResponseBodyUnsupported { .. }
    ));

    let non_json = operation_spec(json!({
        "responses":{"200":{"description":"binary","content":{"application/octet-stream":{}}}},
    }));
    assert!(matches!(
        validation_error(&non_json, "/paths/~1items/get/responses/200"),
        ValidationError::MissingResponseJsonContent { status, .. } if status == "200"
    ));

    for (status, range_error) in [
        ("2xx", false),
        ("success", false),
        ("99", true),
        ("600", true),
    ] {
        let spec = operation_spec(json!({"responses":{(status):{"description":"invalid"}}}));
        let error = validation_error(&spec, &format!("/paths/~1items/get/responses/{status}"));
        if range_error {
            assert!(
                matches!(error, ValidationError::OutOfRangeStatusCode { status_code, .. } if status_code.to_string() == status)
            );
        } else {
            assert!(
                matches!(error, ValidationError::InvalidStatusCode { status: value, .. } if value == status)
            );
        }
    }
}

fn projection_spec(schema: Value, map: bool) -> String {
    let output = if map {
        json!({"unwrap-field":"value","map-field":"label"})
    } else {
        json!({"unwrap-field":"value"})
    };

    operation_spec(json!({
        "x-satay":{"output":output},
        "responses":{"200":{"description":"projected","content":{"application/json":{"schema":schema}}}},
    }))
}

#[test]
fn projection_shape_errors_name_the_real_output_declaration() {
    let pointer = "/paths/~1items/get/x-satay/output";
    let not_envelope = projection_spec(json!({"type":"string"}), false);

    assert!(matches!(
        validation_error(&not_envelope, pointer),
        ValidationError::SatayOutputExpectedObject {
            selector: "unwrap-field",
            ..
        }
    ));

    let missing_field = projection_spec(
        json!({
            "type":"object","properties":{"different":{"type":"string"}},
        }),
        false,
    );

    assert!(matches!(
        validation_error(&missing_field, pointer),
        ValidationError::UnknownSatayOutputField { selector: "unwrap-field", field, .. } if field == "value"
    ));

    let not_array = projection_spec(
        json!({
            "type":"object","properties":{"value":{"type":"string"}},
        }),
        true,
    );

    assert!(matches!(
        validation_error(&not_array, pointer),
        ValidationError::SatayOutputMapRequiresArray { field, .. } if field == "value"
    ));

    let missing_items = projection_spec(
        json!({
            "type":"object","properties":{"value":{"type":"array"}},
        }),
        true,
    );

    assert!(matches!(
        validation_error(&missing_items, pointer),
        ValidationError::MissingArrayItems { .. }
    ));

    let non_object_items = projection_spec(
        json!({
            "type":"object","properties":{"value":{"type":"array","items":{"type":"string"}}},
        }),
        true,
    );

    assert!(matches!(
        validation_error(&non_object_items, pointer),
        ValidationError::SatayOutputExpectedObject {
            selector: "map-field",
            ..
        }
    ));

    let missing_map_field = projection_spec(
        json!({
            "type":"object","properties":{"value":{"type":"array","items":{
                "type":"object","properties":{"different":{"type":"string"}},
            }}},
        }),
        true,
    );

    assert!(matches!(
        validation_error(&missing_map_field, pointer),
        ValidationError::UnknownSatayOutputField { selector: "map-field", field, .. } if field == "label"
    ));
}

#[test]
fn projection_needs_a_selected_json_schema_and_does_not_fall_back_to_alternatives() {
    for responses in [
        json!({"204":{"description":"empty"}}),
        json!({"200":{"description":"schema-less","content":{
            "application/json":{},
            "application/vendor+json":{"schema":{"type":"object","properties":{"value":{"type":"string"}}}},
        }}}),
    ] {
        let spec = operation_spec(json!({
            "x-satay":{"output":{"unwrap-field":"value"}},
            "responses":responses,
        }));
        assert!(matches!(
            validation_error(&spec, "/paths/~1items/get/x-satay/output"),
            ValidationError::SatayOutputRequiresResponseBody { .. }
        ));
    }
}

#[test]
fn projection_configuration_errors_follow_referenced_path_origins() {
    let invalid_field = r#"
openapi: 3.1.0
info: {title: Selector source, version: '1'}
paths:
  /items: {$ref: '#/components/pathItems/Alias'}
components:
  pathItems:
    Alias: {$ref: '#/components/pathItems/Actual'}
    Actual:
      get:
        x-satay: {output: {unwrap-field: absent}}
        responses:
          '200':
            description: envelope
            content:
              application/json:
                schema: {type: object, properties: {actual: {type: string}}}
"#;
    assert!(matches!(
        validation_error(invalid_field, "/components/pathItems/Actual/get/x-satay/output"),
        ValidationError::UnknownSatayOutputField { field, .. } if field == "absent"
    ));
    let invalid_options = operation_spec(json!({
        "x-satay":{"output":{"unwrap-field":""}},
        "responses":{},
    }));
    assert!(matches!(
        validation_error(&invalid_options, "/paths/~1items/get/x-satay/output"),
        ValidationError::InvalidExtension { path, .. } if path == "x-satay.output.unwrap-field"
    ));
}

#[test]
fn invalid_api_key_locations_preserve_structured_error_and_terminal_source() {
    let spec = r#"
openapi: 3.1.0
info: {title: Security, version: '1'}
paths: {}
components:
  securitySchemes:
    Alias: {$ref: '#/components/securitySchemes/Hop'}
    Hop: {$ref: '#/components/securitySchemes/Bad~1~0'}
    'Bad/~': {type: apiKey, name: secret, in: body}
"#;
    match normalize_spec(spec, "test.yaml").expect_err("unsupported API key location") {
        NormalizeError::ApiKeyLocation { value, location } => {
            assert_eq!(value, "body");
            assert_eq!(location.document, "test.yaml");
            assert_eq!(location.pointer, "/components/securitySchemes/Bad~1~0");
        }
        other => panic!("expected API key location error, got {other:?}"),
    }
}
