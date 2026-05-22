use oas3::spec::Spec as OasSpec;

use super::registry::TypeRegistry;
use super::validate::ValidatedDocument;
use crate::error::ValidationError;
use crate::ident::type_ident;
use crate::model::Api;

mod operation;
mod schema;

pub(crate) fn lower_document(document: &ValidatedDocument<'_>) -> Result<Api, ValidationError> {
    tracing::debug!("lowering API from resolved document");

    let spec = document.resolved.spec;
    let mut registry = TypeRegistry::default();
    let server_url = parse_server_url(spec);
    let api_key_security_schemes = operation::parse_api_key_security_schemes(&document.resolved)?;

    reserve_component_type_names(document, &mut registry);

    let components = schema::parse_components(document, &mut registry);
    let operations = operation::parse_operations(document, &mut registry);
    let (components, constrained_types) = registry.finish(components);

    Ok(Api::new(
        server_url,
        api_key_security_schemes,
        components,
        constrained_types,
        operations,
    ))
}

fn parse_server_url(spec: &OasSpec) -> String {
    spec.servers
        .first()
        .map(|server| server.url.clone())
        .unwrap_or_default()
}

fn reserve_component_type_names(document: &ValidatedDocument<'_>, registry: &mut TypeRegistry) {
    for component in &document.components {
        registry.reserve(type_ident(&component.schema_name));
    }
}
