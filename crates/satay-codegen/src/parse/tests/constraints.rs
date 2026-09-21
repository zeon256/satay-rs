use super::ast::*;
use super::*;
use syn::{Fields, Item};

/// Extracts single-field tuple-struct names from `types.rs` in declaration
/// order. Every lifted inline constraint renders as a newtype tuple struct,
/// so this recovers the constrained-type ordering the old private model
/// exposed via its register.
fn newtype_names(file: &syn::File) -> Vec<String> {
    file.items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item) => match &item.fields {
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 && is_pub(&item.vis) => {
                    Some(item.ident.to_string())
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn lifts_inline_constraints_into_generated_types() {
    let files = generate_valid(
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

    let types = parse_rust(file(&files, "types.rs"));

    // `Age` lifts both inclusive integer bounds onto a plain `i32` newtype.
    let age = find_struct(&types, "Age");
    assert_tuple_struct(&types, "Age", "i32");
    assert_attr_contains(&age.attrs, "nutype::nutype", "greater_or_equal = 0");
    assert_attr_contains(&age.attrs, "nutype::nutype", "less_or_equal = 130");

    // The nullable string renders as a plain `Option` alias over the lifted
    // constrained newtype.
    let display_name = find_type_alias(&types, "DisplayName");
    assert_eq!(norm(&display_name.ty), norm_str("Option<DisplayNameValue>"));

    // `DisplayNameValue` keeps the inline length constraint; no upper length
    // bound or pattern was specified.
    let display_name_value = find_struct(&types, "DisplayNameValue");
    assert_tuple_struct(&types, "DisplayNameValue", "String");
    assert_attr_contains(
        &display_name_value.attrs,
        "nutype::nutype",
        "len_char_min = 1",
    );
    assert!(!contains_tokens(&display_name_value, "len_char_max"));
    assert!(!contains_tokens(&display_name_value, "pattern"));

    // The inline-constrained tag item lifts its `minLength` onto the item
    // newtype.
    let tag_item = find_struct(&types, "GetUserTagParameterItem");
    assert_tuple_struct(&types, "GetUserTagParameterItem", "String");
    assert_attr_contains(&tag_item.attrs, "nutype::nutype", "len_char_min = 2");
    assert!(!contains_tokens(&tag_item, "len_char_max"));
    assert!(!contains_tokens(&tag_item, "pattern"));

    // The constrained array keeps its `minItems` bound as a predicate over
    // the lifted item newtype; no upper bound was specified.
    let tag = find_struct(&types, "GetUserTagParameter");
    assert_tuple_struct(
        &types,
        "GetUserTagParameter",
        "Vec<GetUserTagParameterItem>",
    );
    assert_attr_contains(
        &tag.attrs,
        "nutype::nutype",
        "predicate = |items|items.len() >= 1",
    );
    assert!(!contains_tokens(&tag, "less_or_equal"));
    assert!(!contains_tokens(&tag, "len_char_max"));

    // NOTE: the old private model asserted its constrained-type register as
    // [DisplayNameValue, GetUserTagParameterItem, GetUserTagParameter]; in the
    // generated `types.rs` those newtypes appear in the same relative order,
    // interleaved with the `Age` integer newtype.
    assert_eq!(
        newtype_names(&types),
        [
            "Age",
            "DisplayNameValue",
            "GetUserTagParameterItem",
            "GetUserTagParameter",
        ]
    );

    // The query parameter carries the lifted constrained array through its
    // optional wrapper.
    let parts = parse_rust(file(&files, "get_user/parts.rs"));
    let input = find_struct(&parts, "GetUserInput");
    assert_field(input, "tag", "Option<GetUserTagParameter>");
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

    let types = parse_rust(file(&files, "types.rs"));

    let index = find_type_alias(&types, "Index");
    assert_eq!(norm(&index.ty), norm_str("u32"));

    let big_count = find_type_alias(&types, "BigCount");
    assert_eq!(norm(&big_count.ty), norm_str("u64"));

    // Explicit format keeps the u32 base (no single-bound widening to u64)
    // while the bound becomes a validation newtype.
    let floored = find_struct(&types, "FlooredIndex");
    assert_tuple_struct(&types, "FlooredIndex", "u32");
    assert_attr_contains(&floored.attrs, "nutype::nutype", "greater_or_equal = 5");
    assert!(!contains_tokens(&floored, "less_or_equal"));

    // Explicit format wins over dual-bound narrowing: the base stays u32.
    let bounded = find_struct(&types, "BoundedIndex");
    assert_tuple_struct(&types, "BoundedIndex", "u32");
    assert_attr_contains(&bounded.attrs, "nutype::nutype", "less_or_equal = 10");
    assert!(!contains_tokens(&bounded, "greater_or_equal"));
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
