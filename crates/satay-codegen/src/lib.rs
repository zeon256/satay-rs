#![forbid(unsafe_code)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::doc_markdown,
    clippy::elidable_lifetime_names,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::needless_pass_by_value,
    clippy::ref_option,
    clippy::redundant_closure_for_method_calls,
    clippy::single_match_else,
    clippy::struct_field_names,
    clippy::trivially_copy_pass_by_ref,
    clippy::unnecessary_wraps
)]

mod error;
mod parse;

pub use error::{Error, ParseError, ValidationError};
pub use satay_codegen_rust::{GenerateOptions, GeneratedFile, RootModule};
use tracing::info;

/// Generates Rust files from an OpenAPI specification.
///
/// # Errors
///
/// Returns an error if the specification cannot be parsed, validated, or rendered.
#[tracing::instrument(err)]
pub fn generate(spec: &str) -> Result<Vec<GeneratedFile>, Error> {
    generate_with(spec, GenerateOptions::default())
}

/// Generates Rust files from an OpenAPI specification with explicit options.
///
/// # Errors
///
/// Returns an error if the specification cannot be parsed, validated, or rendered.
#[tracing::instrument(err)]
pub fn generate_with(spec: &str, options: GenerateOptions) -> Result<Vec<GeneratedFile>, Error> {
    info!("parsing OpenAPI document");
    parse::generate_api(spec, options)
}
