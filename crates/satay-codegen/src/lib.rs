#![forbid(unsafe_code)]

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

#[tracing::instrument(err)]
pub fn generate(spec: &str) -> Result<Vec<GeneratedFile>, Error> {
    generate_with(spec, GenerateOptions::default())
}

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
