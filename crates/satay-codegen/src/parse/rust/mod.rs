//! Rust validation and lowering from the owned semantic contract.
use crate::model;
mod constraint;
#[derive(Debug, thiserror::Error)]
pub(in crate::parse) enum LowerError {
    #[error(transparent)]
    Rust(#[from] crate::ValidationError),
    #[error(transparent)]
    Frontend(#[from] satay_ir::Diagnostic),
}
mod checked;
mod lower;
mod operation;
mod policy;
mod registry;
mod schema;
#[cfg(test)]
mod tests;

/// Produces the existing Rust model without consulting frontend state.
pub(in crate::parse) fn lower_model(api: &satay_ir::Api) -> Result<model::Api, LowerError> {
    let mut schemas = schema::Schemas::new(api);
    let components = schemas.components()?;
    policy::reject_any_of_cycles(&components)?;
    let operations = operation::operations(api, &mut schemas)?;
    policy::validate_coordinate_uses(&components, &operations)?;
    let tags = api
        .http()
        .tags
        .iter()
        .map(|tag| (tag.name.clone(), tag.description.clone()))
        .collect::<Vec<_>>();
    let model = lower::lower_parts(
        api.http()
            .servers
            .first()
            .map(|server| server.url.clone())
            .unwrap_or_default(),
        operation::security_schemes(api),
        &tags,
        &components,
        &operations,
    )?;
    Ok(model)
}
