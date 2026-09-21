#![allow(
    clippy::match_wildcard_for_single_variants,
    clippy::needless_raw_string_hashes,
    clippy::too_many_lines
)]

use crate::Error;
use crate::error::ValidationError;
use crate::parse::normalize;
use satay_codegen_rust::GeneratedFile;

mod all_of;
mod ast;
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

/// Generates Rust files from a valid spec through the public facade route.
fn generate_valid(spec: &str) -> Vec<GeneratedFile> {
    crate::generate_with(spec, satay_codegen_rust::GenerateOptions::default())
        .expect("valid spec generates")
}

/// Looks up a generated file by its relative output path.
fn file<'a>(files: &'a [GeneratedFile], relative_path: &str) -> &'a GeneratedFile {
    files
        .iter()
        .find(|file| file.relative_path == relative_path)
        .unwrap_or_else(|| panic!("missing generated file {relative_path}"))
}

/// Normalizes a spec into the owned semantic IR for frontend fact assertions.
fn normalize_spec(spec: &str) -> satay_ir::Api {
    normalize::normalize_for_rust(spec, "test.yaml").expect("valid spec normalizes")
}

fn parse_invalid(spec: &str) -> ValidationError {
    match crate::generate(spec).expect_err("OpenAPI must be rejected") {
        Error::Validation(error) => error,
        error => panic!("expected validation error, got {error:?}"),
    }
}
