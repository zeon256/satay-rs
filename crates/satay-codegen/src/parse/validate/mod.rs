pub(crate) mod constraint;

use super::resolve::ResolvedDocument;
use crate::error::ValidationError;

pub(crate) fn validate_document(document: &ResolvedDocument<'_>) -> Result<(), ValidationError> {
    let openapi = document.spec.openapi.as_str();

    if !is_supported_openapi_version(openapi) {
        return Err(ValidationError::UnsupportedOpenApiVersion {
            version: openapi.to_owned(),
        });
    }

    Ok(())
}

fn is_supported_openapi_version(version: &str) -> bool {
    version.starts_with("3.1.")
}
