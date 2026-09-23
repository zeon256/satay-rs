use super::ast::*;
use super::parse_invalid;
use crate::{Error, ValidationError};

#[test]
fn backend_rejection_order_is_preserved_through_the_public_route() {
    let spec = "openapi: 3.1.0\ninfo: {title: Ordering, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    First: {type: integer, format: custom}\n    Second: {type: string, minLength: 5, maxLength: 2}\n";
    assert_eq!(
        crate::generate(spec).unwrap_err().to_string(),
        "schema `First` uses unsupported integer format `custom`"
    );
}

#[test]
fn deferred_extension_errors_retain_their_payloads() {
    for (extension, option) in [
        ("{parse-as: 42}", "parse-as"),
        ("{none-if: false}", "none-if"),
        ("{integer-type: nope}", "integer-type"),
        ("{ignore: [true]}", "ignore"),
    ] {
        let spec = format!(
            "openapi: 3.1.0\ninfo: {{title: Errors, version: '1'}}\npaths: {{}}\ncomponents:\n  schemas:\n    Record:\n      type: object\n      properties:\n        value: {{type: string, x-satay: {extension}}}\n"
        );
        let ValidationError::InvalidExtension {
            context,
            path,
            source,
        } = parse_invalid(&spec)
        else {
            panic!("expected a typed extension error")
        };
        assert_eq!(context, "property `Record.value`");
        assert_eq!(path, format!("x-satay.{option}"));
        assert!(source.is_data());
        assert_eq!((source.line(), source.column()), (0, 0));
    }
}

#[test]
fn first_error_follows_legacy_encounter_order() {
    let rust_error = "{type: integer, format: custom}";
    let semantic_error = "{type: string, minLength: 5, maxLength: 2}";
    for (first, second) in [(rust_error, semantic_error), (semantic_error, rust_error)] {
        for (index, body) in [
            format!(
                "paths: {{}}\ncomponents:\n  schemas:\n    First: {first}\n    Second: {second}"
            ),
            format!(
                "paths: {{}}\ncomponents:\n  schemas:\n    Record:\n      type: object\n      properties:\n        first: {first}\n        second: {second}"
            ),
            format!(
                "paths:\n  /first:\n    get:\n      parameters:\n        - {{name: first, in: query, schema: {first}}}\n      requestBody:\n        content:\n          application/json:\n            schema: {second}\n      responses:\n        oops: {{description: invalid status}}"
            ),
            format!(
                "paths:\n  /first:\n    post:\n      requestBody:\n        content:\n          application/json:\n            schema: {first}\n      responses:\n        '200':\n          description: body\n          content:\n            application/json:\n              schema: {second}"
            ),
            format!(
                "paths:\n  /first:\n    get:\n      responses:\n        '200':\n          description: first\n          content:\n            application/json:\n              schema: {first}\n        '201':\n          description: second\n          content:\n            application/json:\n              schema: {second}"
            ),
        ].into_iter().enumerate() {
            let spec = format!("openapi: 3.1.0\ninfo: {{title: Order, version: '1'}}\n{body}\n");
            let context = [
                "schema `First`",
                "property `Record.first`",
                "parameter `first`",
                "operation `post_first` requestBody",
                "operation `get_first` responses 200 schema",
            ][index];
            let expected = if first == rust_error {
                ValidationError::UnsupportedIntegerFormat { context: context.to_owned(), format: "custom".to_owned() }
            } else {
                ValidationError::InvalidStringLengthBounds { context: context.to_owned(), min_length: 5, max_length: 2 }
            };
            let actual = parse_invalid(&spec);
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
            assert_eq!(actual.to_string(), expected.to_string());
        }
    }
    // Resolution still wins over version checking, even in skipped operations.
    let error = parse_invalid(
        "openapi: 3.0.3\ninfo: {title: Order, version: '1'}\npaths:\n  /skip:\n    get:\n      x-satay: {skip: true}\n      responses:\n        '200': {$ref: '#/components/responses/Missing'}\n",
    );
    let ValidationError::ResolveReference {
        reference, source, ..
    } = error
    else {
        panic!("resolution must precede version checking")
    };
    assert_eq!(reference, "#/components/responses/Missing");
    assert!(
        matches!(*source, ValidationError::MissingJsonPointerToken { token } if token == "Missing")
    );
    assert!(matches!(
        parse_invalid("openapi: 3.1.0\ninfo: {title: Order, version: '1'}\n"),
        ValidationError::MissingPaths
    ));
    for spec in ["not: [valid", "{}"] {
        let error = crate::generate(spec).unwrap_err();
        assert!(matches!(error, Error::Parse(_)), "{spec}: {error:?}");
    }
}

#[test]
fn deferred_diagnostics_restore_nested_payloads_without_parsing_messages() {
    use crate::parse::diagnostic;
    let source = serde_json::from_value::<bool>(serde_json::json!("not a boolean")).unwrap_err();
    let original = ValidationError::ResolveReference {
        reference: "#/components/schemas/Alias".to_owned(),
        context: "property `Record.value`".to_owned(),
        source: Box::new(ValidationError::InvalidExtension {
            context: "schema `Target`".to_owned(),
            path: "x-satay.ignore".to_owned(),
            source,
        }),
    };
    let mut retained = diagnostic::retain(&original);
    retained.message = "Display text is not a serialization format".to_owned();
    let restored = diagnostic::restore(retained);
    assert_eq!(format!("{original:?}"), format!("{restored:?}"));
    assert_eq!(original.to_string(), restored.to_string());
    let ValidationError::ResolveReference {
        reference, source, ..
    } = restored
    else {
        panic!("reference error must retain its variant")
    };
    assert_eq!(reference, "#/components/schemas/Alias");
    let ValidationError::InvalidExtension { path, source, .. } = *source else {
        panic!("extension error must retain its nested variant")
    };
    assert_eq!(path, "x-satay.ignore");
    assert!(source.is_data());
    assert_eq!((source.line(), source.column()), (0, 0));
}

#[test]
fn projection_uses_declared_presence_even_with_unselected_wire_constraints() {
    for wrapper_extra in ["", "minProperties: 1,", "unevaluatedProperties: false,"] {
        let spec = format!(
            r#"
openapi: 3.1.0
info: {{title: Projection, version: '1'}}
paths:
  /value:
    get:
      operationId: value
      x-satay: {{output: {{unwrap-field: value}}}}
      responses:
        '200':
          description: value
          content:
            application/json:
              schema: {{type: object, {wrapper_extra} properties: {{value: {{type: string}}}}}}
"#
        );
        crate::generate(&spec).unwrap();
        let files = super::generate_valid(&spec);
        let json = parse_rust(super::file(&files, "value/json.rs"));
        let decode = find_fn(&json, "decode_value_response");
        // The unwrapped wire field stays an optional projected value even
        // when the wrapper schema carries unselected constraints.
        let tokens = norm(decode);
        assert!(tokens.contains("from_projected_json_slice"));
        assert!(tokens.contains(
            "Option < < S as satay_runtime : : storage : : Storage > : : Text < ' storage > >"
        ));
        assert!(tokens.contains("\"value\""));
    }
}
