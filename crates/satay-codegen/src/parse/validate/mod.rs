pub(crate) mod constraint;
mod index;
mod operation;
mod satay;
mod schema;
mod state;

use super::resolve::ResolvedDocument;
use crate::error::ValidationError;
pub(crate) use state::ValidatedSchemas;

#[derive(Debug)]
pub(crate) struct ValidatedDocument<'a> {
    pub(crate) resolved: ResolvedDocument<'a>,
    pub(crate) schemas: ValidatedSchemas,
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

    let schema_index = index::index_document(&document)?;
    let mut schemas = ValidatedSchemas::new(schema_index);

    schema::validate_components(&document, &mut schemas)?;
    operation::validate_operations(&document, &mut schemas)?;

    Ok(ValidatedDocument {
        resolved: document,
        schemas,
    })
}

fn is_supported_openapi_version(version: &str) -> bool {
    version.starts_with("3.1.")
}
