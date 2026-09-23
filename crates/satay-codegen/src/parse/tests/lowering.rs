use quote::ToTokens;

use super::ast::*;
use super::*;
use syn::{Item, ItemStruct};

/// Extracts top-level type item names (structs, enums, type aliases) in order.
fn type_item_names(file: &syn::File) -> Vec<String> {
    file.items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item) => Some(item.ident.to_string()),
            Item::Enum(item) => Some(item.ident.to_string()),
            Item::Type(item) => Some(item.ident.to_string()),
            _ => None,
        })
        .collect()
}

/// Extracts names of structs rendered as validation newtypes (`nutype::nutype`).
fn nutype_struct_names(file: &syn::File) -> Vec<String> {
    file.items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item)
                if item
                    .attrs
                    .iter()
                    .any(|attr| norm(&attr.path()) == norm_str("nutype::nutype")) =>
            {
                Some(item.ident.to_string())
            }
            _ => None,
        })
        .collect()
}

/// Whether a field carries the serde optional marker
/// (`serde(default, skip_serializing_if = "Option::is_none")`).
fn field_is_optional(item: &ItemStruct, name: &str) -> bool {
    attr_contains(
        &field(item, name).attrs,
        r#"skip_serializing_if = "Option::is_none""#,
    )
}

/// Whether any attribute contains `fragment` (normalized tokens).
fn attr_contains(attrs: &[syn::Attribute], fragment: &str) -> bool {
    attrs.iter().any(|attr| contains_tokens(attr, fragment))
}

/// Asserts that `needles` appear in the given order within `haystack`
/// (normalized token text).
fn token_order(haystack: &impl ToTokens, needles: &[&str]) {
    let text = norm(haystack);
    let mut cursor = 0;
    for needle in needles {
        let pos = text
            .find(&norm_str(needle))
            .unwrap_or_else(|| panic!("fragment `{needle}` not found in generated output"));
        assert!(pos >= cursor, "fragment `{needle}` appears out of order");
        cursor = pos;
    }
}

#[test]
fn lowers_inline_constrained_enum_and_range_schemas_to_ir() {
    let files = generate_valid(INLINE_CONSTRAINED_ENUM_RANGE);
    let types = parse_rust(file(&files, "types.rs"));

    let search = find_struct(&types, "Search");
    assert_eq!(field_names(search), ["code", "state", "window"]);
    assert_field(search, "code", "SearchCode");
    assert_field(search, "state", "SearchState");
    assert_field(search, "window", "SearchWindow");

    let state = find_enum(&types, "SearchState");
    // NOTE: `EnumFallback::None` is validated through the emitted variant list;
    // no additional unknown-value variant is synthesized.
    assert_eq!(variant_names(state), ["Open", "Closed"]);
    assert!(attr_contains(
        &variant(state, "Open").attrs,
        r#"serde(rename = "open")"#
    ));
    assert!(attr_contains(
        &variant(state, "Closed").attrs,
        r#"serde(rename = "closed")"#
    ));

    let window = find_struct(&types, "SearchWindow");
    assert_field(window, "min", "Option<u8>");
    assert_field(window, "max", "Option<u8>");
    assert!(
        contains_tokens(&types, "parse_range::<u8>"),
        "range scalar must decode as u8"
    );

    // NOTE: `api.constrained_types` is expressed through the single emitted
    // validation newtype carrying its string length validation.
    assert_eq!(nutype_struct_names(&types), ["SearchCode"]);
    let code = find_struct(&types, "SearchCode");
    assert_tuple_struct(&types, "SearchCode", "String");
    assert_attr_contains(&code.attrs, "nutype::nutype", "len_char_min = 2");
    assert_attr_contains(&code.attrs, "nutype::nutype", "len_char_max = 8");
}

#[test]
fn parses_components_operations_and_json_media_types_into_ir() {
    let files = generate_valid(
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

    let mod_file = parse_rust(file(&files, "mod.rs"));
    let server_url = find_const(&mod_file, "SERVER_URL");
    assert!(contains_tokens(
        &server_url,
        r#""https://api.example.test/v1""#
    ));

    // NOTE: the component count (3) and the absence of constrained types are
    // expressed through the complete emitted type-item list.
    let types = parse_rust(file(&files, "types.rs"));
    assert_eq!(
        type_item_names(&types),
        ["UpdateUserRequest", "User", "UserStatus"]
    );
    assert!(nutype_struct_names(&types).is_empty());

    let update_user_request = find_struct(&types, "UpdateUserRequest");
    assert_eq!(field_names(update_user_request), ["name"]);
    assert_field(
        update_user_request,
        "name",
        "<S as satay_runtime::storage::Storage>::Text<'storage>",
    );
    assert!(
        !field_is_optional(update_user_request, "name"),
        "required property must not carry the serde optional marker"
    );

    let status = find_enum(&types, "UserStatus");
    assert_eq!(variant_names(status), ["Active", "Suspended"]);
    assert!(attr_contains(
        &variant(status, "Active").attrs,
        r#"serde(rename = "active")"#
    ));
    assert!(attr_contains(
        &variant(status, "Suspended").attrs,
        r#"serde(rename = "suspended")"#
    ));

    let user = find_struct(&types, "User");
    assert_eq!(field_names(user), ["id", "status", "age"]);
    assert_field(
        user,
        "id",
        "<S as satay_runtime::storage::Storage>::Text<'storage>",
    );
    assert_field(user, "status", "UserStatus");
    assert_field(user, "age", "Option<i64>");
    assert!(
        !field_is_optional(user, "id") && !field_is_optional(user, "status"),
        "required properties must not carry the serde optional marker"
    );
    assert!(field_is_optional(user, "age"));

    let api_file = parse_rust(file(&files, "api.rs"));
    let api_struct = find_struct(&api_file, "Api");
    // The two api-key schemes render as builder fields; the bearer scheme does
    // not. Declaration order is preserved.
    assert_eq!(
        field_names(api_struct),
        ["base_url", "account_key", "api_key", "__satay_storage"]
    );
    assert!(contains_tokens(
        &field(api_struct, "account_key"),
        "Option<String>"
    ));
    assert!(contains_tokens(
        &field(api_struct, "api_key"),
        "Option<String>"
    ));
    let apply = find_method(&api_file, "Api", "apply");
    assert!(
        contains_tokens(&apply, r#""AccountKey""#),
        "header api key must use its wire name"
    );
    assert!(
        contains_tokens(&apply, r#""api_key""#),
        "query api key must use its wire name"
    );

    let parts = parse_rust(file(&files, "get_user/parts.rs"));
    let input = find_struct(&parts, "GetUserInput");
    assert_eq!(
        field_names(input),
        ["user_id", "body", "include_details", "body_2"]
    );
    assert_field(
        input,
        "user_id",
        "<S as satay_runtime::storage::Storage>::Text<'storage>",
    );
    assert_field(input, "body", "Option<i32>");
    assert_field(input, "include_details", "bool");
    assert_field(input, "body_2", "UpdateUserRequest<'storage, S>");
    // NOTE: the serde optional marker exists only on model structs; parameter
    // requiredness is expressed by `Option`-wrapping (optional parameters get a
    // builder method, required ones are constructor-only).
    assert!(has_method(&parts, "GetUserInput", "body"));
    for name in ["user_id", "include_details", "body_2"] {
        assert!(
            !has_method(&parts, "GetUserInput", name),
            "required parameter `{name}` must be constructor-only"
        );
    }

    let new = find_method(&parts, "GetUserInput", "new");
    token_order(
        new,
        &[
            "user_id: impl Into<String>",
            "include_details: bool",
            "body_2: UpdateUserRequest<'static, satay_runtime::storage::AllocStorage>",
        ],
    );

    // NOTE: path wire segments are rendered positionally; the path parameter's
    // wire name is not retained beyond the declared template.
    let parts_fn = find_fn(&parts, "get_user_parts");
    token_order(
        parts_fn,
        &["uri.push_str(\"/users/\")", "append_path_segment"],
    );
    assert!(contains_tokens(&parts_fn, r#""includeDetails""#,));
    assert!(contains_tokens(
        &parts_fn,
        r#""application/vnd.acme.user+json; charset=utf-8""#
    ));

    let json_file = parse_rust(file(&files, "get_user/json.rs"));
    let decode = find_fn(&json_file, "decode_get_user_response");
    let response = find_enum(&parts, "GetUserResponse");
    assert_eq!(
        variant_names(response),
        ["Ok", "NotFound", "UnexpectedStatus"]
    );
    assert_doc(&variant(response, "Ok").attrs, "Found");
    assert_doc(&variant(response, "NotFound").attrs, "Missing");
    token_order(
        decode,
        &[
            "200 =>",
            "from_json_slice::<User<'storage, S>>(body)",
            "404 =>",
            "GetUserResponse::<S>::NotFound",
            "_ =>",
        ],
    );
}

#[test]
fn lowers_alias_refs_before_rendering() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let reading = find_struct(&types, "Reading");
    assert_eq!(field_names(reading), ["id", "nickname"]);
    assert_field(reading, "id", "u32");
    assert_field(
        reading,
        "nickname",
        "Option<<S as satay_runtime::storage::Storage>::Text<'storage>>",
    );
    // NOTE: the codec string is a string literal inside the serde attribute, so
    // the check is token-level against the quoted literal.
    assert!(
        contains_tokens(&field(reading, "id"), r#""serde_string::as_u32""#),
        "alias id must decode through the u32 string codec"
    );

    let reading_id = find_type_alias(&types, "ReadingId");
    assert!(contains_tokens(&reading_id, "u32"));
    let optional_name = find_type_alias(&types, "OptionalName");
    assert!(contains_tokens(
        &optional_name,
        "Option<<S as satay_runtime::storage::Storage>::Text<'storage>>"
    ));

    let parts = parse_rust(file(&files, "get_reading/parts.rs"));
    let input = find_struct(&parts, "GetReadingInput");
    // The non-`Option` parameter field expresses requiredness.
    assert_field(input, "reading_id", "u32");
}

#[test]
fn infers_and_overrides_integer_types() {
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

    let types = parse_rust(file(&files, "types.rs"));

    let direction = find_struct(&types, "Direction");
    assert_tuple_struct(&types, "Direction", "i32");
    assert_attr_contains(&direction.attrs, "nutype::nutype", "greater_or_equal = 1");
    assert_attr_contains(&direction.attrs, "nutype::nutype", "less_or_equal = 2");

    let byte = find_struct(&types, "Byte");
    assert_tuple_struct(&types, "Byte", "i64");
    assert_attr_contains(&byte.attrs, "nutype::nutype", "greater_or_equal = 0");
    assert_attr_contains(&byte.attrs, "nutype::nutype", "less_or_equal = 255");

    let legacy_direction = find_struct(&types, "LegacyDirection");
    assert_tuple_struct(&types, "LegacyDirection", "i32");
    assert_attr_contains(
        &legacy_direction.attrs,
        "nutype::nutype",
        "greater_or_equal = 1",
    );
    assert_attr_contains(
        &legacy_direction.attrs,
        "nutype::nutype",
        "less_or_equal = 2",
    );

    let unbounded = find_type_alias(&types, "Unbounded");
    assert!(contains_tokens(&unbounded, "i32"));
}

#[test]
fn parses_type_array_nullability_and_numeric_exclusive_bounds() {
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
    OptionalName:
      type: [string, "null"]
      minLength: 1
    Window:
      type: integer
      exclusiveMinimum: 0
      exclusiveMaximum: 10
"#,
    );

    let types = parse_rust(file(&files, "types.rs"));

    let optional_name = find_type_alias(&types, "OptionalName");
    assert!(contains_tokens(&optional_name, "Option<OptionalNameValue>"));
    let optional_name_value = find_struct(&types, "OptionalNameValue");
    assert_tuple_struct(&types, "OptionalNameValue", "String");
    assert_attr_contains(
        &optional_name_value.attrs,
        "nutype::nutype",
        "len_char_min = 1",
    );

    let window = find_struct(&types, "Window");
    assert_tuple_struct(&types, "Window", "u8");
    assert_attr_contains(&window.attrs, "nutype::nutype", "greater = 0");
    assert_attr_contains(&window.attrs, "nutype::nutype", "less = 10");
}

#[test]
fn lowers_const_string_property_to_singleton_enum() {
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

    let types = parse_rust(file(&files, "types.rs"));

    let cache_control = find_struct(&types, "CacheControl");
    assert_eq!(field_names(cache_control), ["r#type"]);
    assert_field(cache_control, "r#type", "CacheControlType");

    let cache_control_type = find_enum(&types, "CacheControlType");
    assert_eq!(variant_names(cache_control_type), ["Ephemeral"]);
    assert!(attr_contains(
        &variant(cache_control_type, "Ephemeral").attrs,
        r#"serde(rename = "ephemeral")"#
    ));
    let as_str = find_method(&types, "CacheControlType", "as_str");
    assert!(contains_tokens(
        &as_str,
        r#"Self::Ephemeral => "ephemeral""#
    ));
}

#[test]
fn lowers_single_ref_nullable_any_of_to_option_of_ref() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));
    let user = find_struct(&types, "User");
    assert_field(user, "profile", "Option<Profile<'storage, S>>");

    let profile = find_struct(&types, "Profile");
    assert_eq!(field_names(profile), ["id"]);

    assert!(
        !contains_ident(&types, "UserProfile"),
        "single-reference untagged union must not synthesize a wrapper component"
    );
}

#[test]
fn lowers_wrapped_single_ref_nullable_any_of_to_option_of_ref() {
    let files = generate_valid(
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
            - description: The user's profile.
              allOf:
                - $ref: '#/components/schemas/Profile'
            - type: 'null'
"##,
    );

    let types = parse_rust(file(&files, "types.rs"));
    let user = find_struct(&types, "User");
    assert_field(user, "profile", "Option<Profile<'storage, S>>");

    let profile = find_struct(&types, "Profile");
    assert_eq!(field_names(profile), ["id"]);

    assert!(
        !contains_ident(&types, "UserProfile"),
        "single-reference untagged union must not synthesize a wrapper component"
    );
}

#[test]
fn parses_wildcard_response_range_after_exact_statuses() {
    let files = generate_valid(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /users:
    get:
      operationId: getUser
      responses:
        '4XX':
          description: Client error
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'
        '200':
          description: Found user
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
        '404':
          description: Not found
components:
  schemas:
    User:
      type: object
      required:
        - id
      properties:
        id:
          type: string
    ErrorResponse:
      type: object
      required:
        - message
      properties:
        message:
          type: string
"#,
    );

    let parts = parse_rust(file(&files, "get_user/parts.rs"));
    let response = find_enum(&parts, "GetUserResponse");
    assert_eq!(
        variant_names(response),
        ["Ok", "NotFound", "ClientError", "UnexpectedStatus"]
    );
    assert_doc(&variant(response, "Ok").attrs, "Found user");
    assert_doc(&variant(response, "NotFound").attrs, "Not found");
    assert_doc(&variant(response, "ClientError").attrs, "Client error");
    assert!(
        contains_tokens(
            &variant(response, "ClientError"),
            "http::StatusCode, ErrorResponse<'storage, S>"
        ),
        "range responses carry their concrete status and projected body"
    );

    let json_file = parse_rust(file(&files, "get_user/json.rs"));
    let decode = find_fn(&json_file, "decode_get_user_response");
    token_order(
        decode,
        &[
            "200 =>",
            "from_json_slice::<User<'storage, S>>(body)",
            "404 =>",
            "GetUserResponse::<S>::NotFound",
            "400..=499 =>",
            "from_json_slice::<ErrorResponse<'storage, S>>(body)",
            "GetUserResponse::<S>::ClientError(status, value)",
            "_ =>",
        ],
    );
}

#[test]
fn folds_nullable_optional_query_and_header_parameters() {
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
      parameters:
        - name: q
          in: query
          required: false
          schema:
            anyOf:
              - type: string
              - type: "null"
        - name: page
          in: query
          schema:
            oneOf:
              - type: string
              - type: "null"
        - name: limit
          in: query
          schema:
            type: [integer, "null"]
        - name: x-trace
          in: header
          schema:
            anyOf:
              - type: string
              - type: "null"
        - name: count
          in: query
          schema:
            anyOf:
              - type: integer
                minimum: 1
              - type: "null"
      responses:
        '204':
          description: No content
"#,
    );

    // NOTE: parameter-level IR facts (plain `T`, `I64`, constrained inner) are
    // expressed through the `Option`-wrapped builder input fields and the
    // emitted validation newtype.
    let parts = parse_rust(file(&files, "ping/parts.rs"));
    let input = find_struct(&parts, "PingInput");
    assert_eq!(
        field_names(input),
        ["q", "page", "limit", "x_trace", "count"]
    );
    assert_field(
        input,
        "q",
        "Option<<S as satay_runtime::storage::Storage>::Text<'storage>>",
    );
    assert_field(
        input,
        "page",
        "Option<<S as satay_runtime::storage::Storage>::Text<'storage>>",
    );
    assert_field(input, "limit", "Option<i64>");
    assert_field(
        input,
        "x_trace",
        "Option<<S as satay_runtime::storage::Storage>::Text<'storage>>",
    );
    assert_field(input, "count", "Option<PingCountParameter>");
    for name in ["q", "page", "limit", "x_trace", "count"] {
        assert!(
            has_method(&parts, "PingInput", name),
            "folded parameter `{name}` must keep a builder method"
        );
    }

    let parts_fn = find_fn(&parts, "ping_parts");
    for wire_name in ["q", "page", "limit", "count"] {
        assert!(
            contains_tokens(&parts_fn, &format!(r#""{wire_name}""#)),
            "query parameter `{wire_name}` must use its wire name"
        );
    }
    assert!(
        contains_tokens(&parts_fn, r#""x-trace""#),
        "header parameter must use its wire name"
    );
    token_order(
        parts_fn,
        &["append_query_pair", "insert_header", "\"x-trace\""],
    );

    // Constraints on the peeled branch survive as a validation newtype.
    let types = parse_rust(file(&files, "types.rs"));
    let count_parameter = find_struct(&types, "PingCountParameter");
    assert_tuple_struct(&types, "PingCountParameter", "u64");
    assert_attr_contains(
        &count_parameter.attrs,
        "nutype::nutype",
        "greater_or_equal = 1",
    );
}

#[test]
fn rejects_required_nullable_query_parameter() {
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
        - name: q
          in: query
          required: true
          schema:
            anyOf:
              - type: string
              - type: "null"
      responses:
        '204':
          description: No content
"#,
    );

    match err {
        ValidationError::NullableParameterUnsupported { wire_name } => {
            assert_eq!(wire_name, "q");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_nullable_optional_parameter_with_union_constraint_sibling() {
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
        - name: count
          in: query
          schema:
            anyOf:
              - type: integer
              - type: "null"
            minimum: 1
      responses:
        '204':
          description: No content
"#,
    );

    match err {
        ValidationError::UnsupportedAnyOfSiblingKeyword { context, keyword } => {
            assert_eq!(context, "parameter `count`");
            assert_eq!(keyword, "minimum");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn rejects_optional_query_parameter_with_non_null_union() {
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
        - name: q
          in: query
          schema:
            anyOf:
              - type: string
              - type: integer
      responses:
        '204':
          description: No content
"#,
    );

    match err {
        ValidationError::AnyOfParameterUnsupported { wire_name } => {
            assert_eq!(wire_name, "q");
        }
        other => panic!("unexpected error: {other}"),
    }
}
