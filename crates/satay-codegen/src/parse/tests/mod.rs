#![allow(
    clippy::match_wildcard_for_single_variants,
    clippy::needless_raw_string_hashes,
    clippy::too_many_lines
)]

use crate::parse::{
    normalize::{self, NormalizeError},
    rust,
};
use crate::{RootModule, render};

use crate::error::ValidationError;
use crate::model::{
    Api, ApiKeyLocation, Component, ComponentKind, EnumFallback, Field, HttpMethod, IntegerLimit,
    IntegerType, Operation, Parameter, ParameterLocation, ParseAs, PathSegment, RangeScalar,
    RangeTypeRef, ResponseStatus, StringCodec, TypeRef, UnionTagStyle, Validation,
};
use crate::parse::{parse_api, parse_document};

mod all_of;
mod constraints;
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
    let document = parse_document(spec).expect("document parses");
    let legacy = parse_api(&document).expect("OpenAPI validates");
    let semantic = normalize::normalize_for_rust(spec, "test.yaml").expect("valid spec normalizes");
    for root_module in [RootModule::ModRs, RootModule::LibRs] {
        let options = crate::GenerateOptions { root_module };
        let expected = render::render_api(&legacy, options);
        let actual = rust::lower_api(&semantic, options).expect("valid semantic IR lowers");
        assert_eq!(actual.len(), expected.len(), "semantic file count");
        for (actual, expected) in actual.iter().zip(&expected) {
            assert_eq!(actual.relative_path, expected.relative_path);
            if actual.contents != expected.contents {
                let mismatch = actual
                    .contents
                    .lines()
                    .zip(expected.contents.lines())
                    .find(|(a, b)| a != b);
                panic!(
                    "semantic generation parity: {}: {mismatch:?}",
                    actual.relative_path
                );
            }
        }
    }
    legacy
}

fn parse_invalid(spec: &str) -> ValidationError {
    let document = parse_document(spec).expect("document parses");
    let expected = parse_api(&document).expect_err("OpenAPI must be rejected");
    let actual = match normalize::normalize_for_rust(spec, "test.yaml") {
        Ok(api) => rust::lower_api(&api, crate::GenerateOptions::default())
            .expect_err("invalid semantic IR must be rejected")
            .to_string(),
        Err(NormalizeError::Validation { source, .. }) => source.to_string(),
        Err(error) => error.to_string(),
    };
    assert_eq!(actual, expected.to_string(), "semantic diagnostic parity");
    expected
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
