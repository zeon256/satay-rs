//! Rust validation and lowering. The semantic entry is staged privately.
#[cfg(test)]
use crate::model;
pub(in crate::parse) mod constraint;
#[cfg(test)]
#[derive(Debug, thiserror::Error)]
pub(in crate::parse) enum LowerError {
    #[error(transparent)]
    Rust(#[from] crate::ValidationError),
    #[error(transparent)]
    Frontend(#[from] satay_ir::Diagnostic),
}
#[cfg(test)]
mod operation;
pub(in crate::parse) mod policy;
#[cfg(test)]
mod schema;
#[cfg(test)]
mod tests;

/// Private parity entry. The complete source input is the semantic contract.
#[cfg(test)]
pub(in crate::parse) fn lower_api(
    api: &satay_ir::Api,
    options: crate::GenerateOptions,
) -> Result<Vec<crate::GeneratedFile>, LowerError> {
    use crate::render;
    Ok(render::render_api(&lower_model(api)?, options))
}

/// Produces the existing Rust model without consulting frontend state.
#[cfg(test)]
fn lower_model(api: &satay_ir::Api) -> Result<model::Api, LowerError> {
    use crate::parse::lower;
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
