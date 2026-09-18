use oas3::spec::Spec as OasSpec;

use crate::Error;
use crate::error::ParseError;
use crate::model::Api;
use normalize::NormalizeError;
use rust::LowerError;

mod diagnostic;
mod helpers;
mod normalize;
mod reference;
mod resolve;
mod rust;
mod satay;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) struct Document {
    spec: OasSpec,
}

pub(crate) fn semantic_api(spec: &str) -> Result<Api, Error> {
    let api = normalize::normalize_for_rust(spec, "input.yaml").map_err(|error| match error {
        NormalizeError::Parse(error) => Error::Parse(error),
        NormalizeError::Validation { source, .. } => Error::Validation(*source),
        error => Error::Internal {
            message: error.to_string(),
        },
    })?;
    match rust::lower_model(&api) {
        Ok(model) => Ok(model),
        Err(LowerError::Rust(error)) => Err(Error::Validation(error)),
        Err(LowerError::Frontend(error)) => match diagnostic::try_restore(error) {
            Ok(error) => Err(Error::Validation(error)),
            Err(kind) => Err(Error::Internal {
                message: format!("unrepresentable semantic diagnostic: {kind:?}"),
            }),
        },
    }
}

pub(crate) fn parse_document(spec: &str) -> Result<Document, ParseError> {
    let spec = oas3::from_yaml(spec)?;

    Ok(Document { spec })
}
