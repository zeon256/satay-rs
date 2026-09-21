use oas3::spec::Spec as OasSpec;

use crate::Error;
use crate::error::ParseError;
use normalize::NormalizeError;
use satay_codegen_rust::Error as BackendError;
use satay_codegen_rust::GenerateOptions;
use satay_codegen_rust::GeneratedFile;

mod diagnostic;
mod helpers;
mod normalize;
mod reference;
mod resolve;
mod satay;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) struct Document {
    spec: OasSpec,
}

/// Normalizes an OpenAPI document into the owned semantic IR graph.
pub(crate) fn normalize_api(spec: &str) -> Result<satay_ir::Api, Error> {
    normalize::normalize_for_rust(spec, "input.yaml").map_err(|error| match error {
        NormalizeError::Parse(error) => Error::Parse(error),
        NormalizeError::Validation { source, .. } => Error::Validation(*source),
        error => Error::Internal {
            message: error.to_string(),
        },
    })
}

/// Normalizes, lowers through the Rust backend, and translates backend
/// errors into the public facade diagnostics.
pub(crate) fn generate_api(
    spec: &str,
    options: GenerateOptions,
) -> Result<Vec<GeneratedFile>, Error> {
    let api = normalize_api(spec)?;
    match satay_codegen_rust::generate(&api, options) {
        Ok(files) => Ok(files),
        Err(BackendError::Rust(error)) => Err(Error::Validation(error.into())),
        Err(BackendError::Frontend(error)) => match diagnostic::try_restore(error) {
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
