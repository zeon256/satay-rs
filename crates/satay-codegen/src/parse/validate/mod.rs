pub(crate) mod constraint;
mod operation;
mod satay;
mod schema;

use super::resolve::ResolvedDocument;
use crate::error::ValidationError;
pub(crate) use satay::ValidatedSatay;

#[derive(Debug)]
pub(crate) struct ValidatedDocument<'a> {
    pub(crate) resolved: ResolvedDocument<'a>,
    pub(crate) satay: ValidatedSatay,
}

pub(crate) fn validate_document<'a>(
    document: ResolvedDocument<'a>,
) -> Result<ValidatedDocument<'a>, ValidationError> {
    let openapi = document.spec.openapi.as_str();

    if !is_supported_openapi_version(openapi) {
        return Err(ValidationError::UnsupportedOpenApiVersion {
            version: openapi.to_owned(),
        });
    }

    let mut satay = ValidatedSatay::default();

    schema::validate_components(&document, &mut satay)?;
    operation::validate_operations(&document, &mut satay)?;

    Ok(ValidatedDocument {
        resolved: document,
        satay,
    })
}

fn is_supported_openapi_version(version: &str) -> bool {
    version.starts_with("3.1.")
}
