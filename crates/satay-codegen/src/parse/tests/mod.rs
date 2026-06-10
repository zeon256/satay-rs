use crate::error::ValidationError;
use crate::model::{
    Api, ApiKeyLocation, Component, ComponentKind, Field, HttpMethod, IntegerLimit, IntegerType,
    Operation, Parameter, ParameterLocation, ParseAs, PathSegment, RangeScalar, RangeTypeRef,
    TypeRef, Validation,
};
use crate::parse::{parse_api, parse_document};

const INLINE_CONSTRAINED_ENUM_RANGE: &str =
    include_str!("../../../../../tests/fixtures/parse-inline-constrained-enum-range.yaml");

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

mod composition;
mod constraints;
mod errors;
mod extensions;
mod lowering;
mod naming;
