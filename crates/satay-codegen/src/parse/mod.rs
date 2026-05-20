use oas3::spec::Spec as OasSpec;

use crate::error::{ParseError, ValidationError};
use crate::model::Api;

mod helpers;
mod lower;
mod reference;
mod registry;
mod resolve;
mod satay;
#[cfg(test)]
mod tests;
mod validate;

#[derive(Debug)]
pub(crate) struct Document {
    spec: OasSpec,
}

pub(crate) fn parse_api(document: &Document) -> Result<Api, ValidationError> {
    tracing::debug!("parsing API from document");

    let resolved = resolve::resolve_document(document)?;
    validate::validate_document(&resolved)?;
    lower::lower_document(&resolved)
}

pub(crate) fn parse_document(spec: &str) -> Result<Document, ParseError> {
    let spec = oas3::from_yaml(spec)?;

    Ok(Document { spec })
}
