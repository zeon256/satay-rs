use super::checked::{CheckedComponent, CheckedOperation};
use super::error::ValidationError;
use super::registry::TypeRegistry;
use crate::ident::type_ident;
use crate::model::{Api, ApiKeySecurityScheme};

mod operation;
mod schema;

/// Builds the render model from Rust-owned checked values.
pub(crate) fn lower_parts(
    server_url: String,
    api_key_security_schemes: Vec<ApiKeySecurityScheme>,
    tags: &[(String, Option<String>)],
    validated_components: &[CheckedComponent],
    validated_operations: &[CheckedOperation],
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
