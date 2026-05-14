#![forbid(unsafe_code)]

mod error;
mod ident;
mod model;
mod parse;
mod render;

pub use error::Error;

pub fn generate(spec: &str) -> Result<String, Error> {
    let document = parse::parse_document(spec)?;
    let api = parse::parse_api(&document)?;
    render::render_api(&api)
}
