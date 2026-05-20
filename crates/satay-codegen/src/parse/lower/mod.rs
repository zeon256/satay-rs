use oas3::spec::Spec as OasSpec;

use super::registry::TypeRegistry;
use super::resolve::ResolvedDocument;
use crate::error::ValidationError;
use crate::ident::type_ident;
use crate::model::Api;

mod operation;
mod schema;

pub(crate) fn lower_document(document: &ResolvedDocument<'_>) -> Result<Api, ValidationError> {
    tracing::debug!("lowering API from resolved document");

    let spec = document.spec;
    let mut registry = TypeRegistry::default();
    let server_url = parse_server_url(spec);
    let api_key_security_schemes = operation::parse_api_key_security_schemes(document)?;

    reserve_component_type_names(spec, &mut registry);

    let mut components = schema::parse_components(document, &mut registry)?;
    let operations = operation::parse_operations(document, &mut registry)?;
    let constrained_types = registry.finish(&mut components);

    Ok(Api {
        server_url,
        api_key_security_schemes,
        components,
        constrained_types,
        operations,
    })
}

fn parse_server_url(spec: &OasSpec) -> String {
    spec.servers
        .first()
        .map(|server| server.url.clone())
        .unwrap_or_default()
}

fn reserve_component_type_names(spec: &OasSpec, registry: &mut TypeRegistry) {
    let Some(components) = spec.components.as_ref() else {
        return;
    };

    for schema_name in components.schemas.keys() {
        registry.reserve(type_ident(schema_name));
    }
}
