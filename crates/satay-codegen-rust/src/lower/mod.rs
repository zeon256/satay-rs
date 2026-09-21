//! Rust validation and lowering from the owned semantic contract.
use crate::error::Error;
use crate::model;
mod assemble;
mod checked;
mod constraint;
pub(crate) mod error;
mod helpers;
mod operation;
mod policy;
mod registry;
mod schema;
#[cfg(test)]
mod tests;

/// Produces the Rust model without consulting frontend state.
pub(crate) fn lower_model(api: &satay_ir::Api) -> Result<model::Api, Error> {
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
    let model = assemble::lower_parts(
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
