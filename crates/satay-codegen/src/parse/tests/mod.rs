#![allow(
    clippy::match_wildcard_for_single_variants,
    clippy::needless_raw_string_hashes,
    clippy::too_many_lines
)]

use crate::parse::{normalize, rust};

use crate::Error;
use crate::error::ValidationError;
use crate::model::{
    Api, ApiKeyLocation, Component, ComponentKind, EnumFallback, Field, HttpMethod, IntegerLimit,
    IntegerType, Operation, Parameter, ParameterLocation, ParseAs, PathSegment, RangeScalar,
    RangeTypeRef, ResponseStatus, StringCodec, TypeRef, UnionTagStyle, Validation,
};

mod all_of;
mod constraints;
mod cutover;
mod errors;
mod extensions;
mod groups;
mod ir;
mod lowering;
mod maps;
mod naming;
mod unions;

const INLINE_CONSTRAINED_ENUM_RANGE: &str =
    include_str!("../../../../../tests/fixtures/parse-inline-constrained-enum-range.yaml");

fn parse_valid(spec: &str) -> Api {
    let semantic = normalize::normalize_for_rust(spec, "test.yaml").expect("valid spec normalizes");
    rust::lower_model(&semantic).expect("valid semantic IR lowers")
}

fn parse_invalid(spec: &str) -> ValidationError {
    match crate::generate(spec).expect_err("OpenAPI must be rejected") {
        Error::Validation(error) => error,
        error => panic!("expected validation error, got {error:?}"),
    }
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
