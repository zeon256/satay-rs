#![forbid(unsafe_code)]

mod error;
mod ident;
mod model;
mod parse;
mod render;

pub use error::{Error, ParseError, ValidationError};
pub use render::GeneratedFile;
use tracing::info;

#[tracing::instrument(err)]
pub fn generate(spec: &str) -> Result<Vec<GeneratedFile>, Error> {
    info!("parsing OpenAPI document");
    let document = parse::parse_document(spec)?;
    let api = parse::parse_api(&document)?;
    info!(
        components = api.components.len(),
        operations = api.operations.len(),
        "parsed API"
    );
    Ok(render::render_api(&api))
}
