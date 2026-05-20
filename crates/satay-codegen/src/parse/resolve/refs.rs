use crate::error::ValidationError;

pub(crate) fn local_ref_name(
    reference: &str,
    section: &'static str,
) -> Result<String, ValidationError> {
    let prefix = format!("#/components/{section}/");
    let Some(name) = reference.strip_prefix(&prefix) else {
        return Err(ValidationError::InvalidComponentReference {
            reference: reference.to_owned(),
            section,
        });
    };
    Ok(json_pointer_unescape(name))
}

fn json_pointer_unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}
