pub(crate) mod constraint;
mod operation;
mod satay;
mod schema;
mod state;

use super::normalize::NormalizedDocument;
pub(crate) use super::normalize::SchemaId;
use crate::error::ValidationError;
pub(crate) use state::ValidatedSchemas;

#[derive(Debug)]
pub(crate) struct ValidatedDocument<'a> {
    pub(crate) normalized: NormalizedDocument<'a>,
    pub(crate) schemas: ValidatedSchemas,
}

pub(crate) fn validate_document<'a>(
    document: NormalizedDocument<'a>,
) -> Result<ValidatedDocument<'a>, ValidationError> {
    let openapi = document.resolved.spec.openapi.as_str();

    if !is_supported_openapi_version(openapi) {
        return Err(ValidationError::UnsupportedOpenApiVersion {
            version: openapi.to_owned(),
        });
    }

    let mut schemas = ValidatedSchemas::default();

    schema::validate_components(&document, &mut schemas)?;
    operation::validate_operations(&document, &mut schemas)?;

    Ok(ValidatedDocument {
        normalized: document,
        schemas,
    })
}

fn is_supported_openapi_version(version: &str) -> bool {
    version.starts_with("3.1.")
}
