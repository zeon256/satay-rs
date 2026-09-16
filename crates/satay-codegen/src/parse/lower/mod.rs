use oas3::spec::Spec as OasSpec;
use tracing::debug;

use super::registry::TypeRegistry;
use super::validate::{ValidatedComponent, ValidatedDocument, ValidatedOperation};
use crate::error::ValidationError;
use crate::ident::type_ident;
use crate::model::{Api, ApiKeySecurityScheme};

mod operation;
mod schema;

pub(crate) fn lower_document(document: &ValidatedDocument<'_>) -> Result<Api, ValidationError> {
    debug!("lowering API from resolved document");

    let spec = document.resolved.spec;
    let server_url = parse_server_url(spec);
    let api_key_security_schemes = operation::parse_api_key_security_schemes(&document.resolved)?;
    let tags = spec
        .tags
        .iter()
        .map(|tag| (tag.name.clone(), tag.description.clone()))
        .collect::<Vec<_>>();
    lower_parts(
        server_url,
        api_key_security_schemes,
        &tags,
        &document.components,
        &document.operations,
    )
}

/// Shared Rust model construction; no frontend document escapes the adapter.
pub(in crate::parse) fn lower_parts(
    server_url: String,
    api_key_security_schemes: Vec<ApiKeySecurityScheme>,
    tags: &[(String, Option<String>)],
    validated_components: &[ValidatedComponent],
    validated_operations: &[ValidatedOperation],
) -> Result<Api, ValidationError> {
    let mut registry = TypeRegistry::default();
    for component in validated_components {
        registry.reserve(type_ident(&component.schema_name));
    }

    let mut schemas = schema::SchemaLowerer::new(validated_components);
    let components = schemas.parse_components(&mut registry);
    let operations =
        operation::parse_operations(validated_operations, &mut registry, &mut schemas)?;
    let groups = operation::parse_api_groups(tags, &api_key_security_schemes, &operations);
    let (components, constrained_types) = registry.finish(components);

    Ok(Api::new(
        server_url,
        api_key_security_schemes,
        components,
        constrained_types,
        groups,
        operations,
    ))
}

fn parse_server_url(spec: &OasSpec) -> String {
    spec.servers
        .first()
        .map(|server| server.url.clone())
        .unwrap_or_default()
}
