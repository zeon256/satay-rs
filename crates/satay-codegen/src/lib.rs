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
mod ident;
mod model;
mod parse;
mod render;

pub use error::{Error, ParseError, ValidationError};
pub use render::GeneratedFile;
use tracing::info;

/// Which root module file to emit at the output directory root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RootModule {
    /// `mod.rs` (default).
    #[default]
    ModRs,
    /// `lib.rs` for a generated crate root.
    LibRs,
}

/// Options for code generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GenerateOptions {
    /// Root module filename (`mod.rs` or `lib.rs`).
    pub root_module: RootModule,
}

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
    let document = parse::parse_document(spec)?;
    let api = parse::parse_api(&document)?;
    info!(
        components = api.components.len(),
        operations = api.operations.len(),
        "parsed API"
    );
    Ok(render::render_api(&api, options))
}
