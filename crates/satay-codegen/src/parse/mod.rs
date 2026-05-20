use std::collections::BTreeSet;

use oas3::spec::Spec as OasSpec;

use crate::error::{ParseError, ValidationError};
use crate::ident::{type_ident, unique_ident};
use crate::model::{
    Api, Component, ComponentKind, ConstrainedType, EnumVariant, RangeScalar, RangeType,
    RangeTypeRef, TypeRef, Validation,
};

mod constraint;
mod helpers;
mod operation;
mod reference;
mod satay;
mod schema;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) struct Document {
    spec: OasSpec,
}

#[derive(Debug, Default)]
pub(crate) struct TypeRegistry {
    generated: Vec<ConstrainedType>,
    inline_enums: Vec<Component>,
    inline_ranges: Vec<Component>,
    used_names: BTreeSet<String>,
}

impl TypeRegistry {
    fn reserve(&mut self, rust_name: String) {
        self.used_names.insert(rust_name);
    }

    fn constrained_ref(
        &mut self,
        type_name_hint: &str,
        description: Option<String>,
        inner: TypeRef,
        validation: Validation,
    ) -> TypeRef {
        let rust_name = unique_ident(type_ident(type_name_hint), &mut self.used_names);

        self.generated.push(ConstrainedType {
            rust_name: rust_name.clone(),
            description,
            inner: inner.clone(),
            validation,
        });

        TypeRef::Constrained {
            rust_name,
            inner: Box::new(inner),
        }
    }

    fn inline_enum_ref(
        &mut self,
        type_name_hint: &str,
        description: Option<String>,
        variants: Vec<EnumVariant>,
    ) -> TypeRef {
        let rust_name = unique_ident(type_ident(type_name_hint), &mut self.used_names);

        self.inline_enums.push(Component {
            rust_name: rust_name.clone(),
            description,
            kind: ComponentKind::Enum(variants),
        });

        TypeRef::Named(rust_name)
    }

    fn inline_range_ref(
        &mut self,
        type_name_hint: &str,
        description: Option<String>,
        scalar: RangeScalar,
    ) -> TypeRef {
        let rust_name = unique_ident(type_ident(type_name_hint), &mut self.used_names);

        self.inline_ranges.push(Component {
            rust_name: rust_name.clone(),
            description: description.clone(),
            kind: ComponentKind::Range(RangeType {
                rust_name: rust_name.clone(),
                description,
                scalar,
            }),
        });

        TypeRef::Range(RangeTypeRef { rust_name, scalar })
    }
}

pub(crate) fn parse_api(document: &Document) -> Result<Api, ValidationError> {
    tracing::debug!("parsing API from document");

    let spec = &document.spec;
    let openapi = spec.openapi.as_str();

    if !is_supported_openapi_version(openapi) {
        return Err(ValidationError::UnsupportedOpenApiVersion {
            version: openapi.to_owned(),
        });
    }

    let mut registry = TypeRegistry::default();
    let server_url = parse_server_url(spec);
    let api_key_security_schemes = operation::parse_api_key_security_schemes(document)?;

    reserve_component_type_names(spec, &mut registry);

    let mut components = schema::parse_components(document, &mut registry)?;
    let operations = operation::parse_operations(document, &mut registry)?;

    components.extend(registry.inline_enums);
    components.extend(registry.inline_ranges);

    Ok(Api {
        server_url,
        api_key_security_schemes,
        components,
        constrained_types: registry.generated,
        operations,
    })
}

pub(crate) fn parse_document(spec: &str) -> Result<Document, ParseError> {
    let spec = oas3::from_yaml(spec)?;

    Ok(Document { spec })
}

fn is_supported_openapi_version(version: &str) -> bool {
    version.starts_with("3.1.")
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
