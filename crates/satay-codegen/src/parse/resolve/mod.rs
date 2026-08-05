use oas3::Map as OasMap;
use oas3::spec::{
    ComponentTarget, MediaType as OasMediaType, ObjectOrReference, Operation as OasOperation,
    Parameter as OasParameter, PathItem as OasPathItem, RequestBody as OasRequestBody,
    ResolvableComponent, ResolveError, Resolver, Response as OasResponse, Schema as OasSchema,
    SecurityScheme as OasSecurityScheme, Spec as OasSpec,
};

use super::Document;
use crate::error::ValidationError;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedDocument<'a> {
    pub(crate) spec: &'a OasSpec,
    resolver: Resolver<'a>,
}

impl ResolvedDocument<'_> {
    pub(crate) fn resolve<'a, T>(
        &'a self,
        component: &'a ObjectOrReference<T>,
        context: &str,
    ) -> Result<&'a T, ValidationError>
    where
        T: ResolvableComponent,
    {
        self.resolver.resolve(component).map_err(|source| {
            map_resolve_error::<T>(source, component_reference(component), context)
        })
    }

    pub(crate) fn resolve_schema<'a>(
        &'a self,
        schema: &'a OasSchema,
        context: &str,
    ) -> Result<&'a OasSchema, ValidationError> {
        self.resolver
            .resolve_schema(schema)
            .map_err(|source| map_resolve_error::<OasSchema>(source, schema.reference(), context))
    }

    pub(crate) fn resolve_path_item<'a>(
        &'a self,
        path_item: &'a OasPathItem,
        context: &str,
    ) -> Result<&'a OasPathItem, ValidationError> {
        self.resolver
            .resolve_path_item(path_item)
            .map_err(|source| {
                map_resolve_error::<OasPathItem>(source, path_item.reference.as_deref(), context)
            })
    }

    fn resolve_path_item_component<'a>(
        &'a self,
        path_item: &'a ObjectOrReference<OasPathItem>,
        context: &str,
    ) -> Result<&'a OasPathItem, ValidationError> {
        let path_item = self.resolve(path_item, context)?;
        self.resolve_path_item(path_item, context)
    }
}

pub(crate) fn resolve_document(
    document: &Document,
) -> Result<ResolvedDocument<'_>, ValidationError> {
    let resolved = ResolvedDocument {
        spec: &document.spec,
        resolver: Resolver::new(&document.spec),
    };

    validate_component_refs(&resolved)?;
    validate_path_refs(&resolved)?;

    Ok(resolved)
}

fn component_reference<T>(component: &ObjectOrReference<T>) -> Option<&str> {
    match component {
        ObjectOrReference::Object(_) => None,
        ObjectOrReference::Ref { ref_path, .. } => Some(ref_path),
    }
}

fn map_resolve_error<T>(
    source: ResolveError,
    root_reference: Option<&str>,
    context: &str,
) -> ValidationError
where
    T: ComponentTarget,
{
    let reference = match &source {
        ResolveError::InvalidReference(source) => source.reference().to_owned(),
        ResolveError::MissingComponent { reference, .. } => reference.clone(),
        ResolveError::Cycle { references } => references.first().cloned().unwrap_or_default(),
        _ => root_reference.unwrap_or_default().to_owned(),
    };

    let source = match source {
        ResolveError::InvalidReference(source) => ValidationError::InvalidComponentReference {
            reference: source.reference().to_owned(),
            section: T::COMPONENT_SECTION,
        },
        ResolveError::MissingComponent { name, .. } => {
            ValidationError::MissingJsonPointerToken { token: name }
        }
        ResolveError::Cycle { references } => ValidationError::CircularReference {
            reference: references
                .first()
                .cloned()
                .unwrap_or_else(|| reference.clone()),
        },
        _ => ValidationError::InvalidComponentReference {
            reference: reference.clone(),
            section: T::COMPONENT_SECTION,
        },
    };

    ValidationError::ResolveReference {
        reference,
        context: context.to_owned(),
        source: Box::new(source),
    }
}

fn validate_component_refs(document: &ResolvedDocument<'_>) -> Result<(), ValidationError> {
    let Some(components) = document.spec.components.as_ref() else {
        return Ok(());
    };

    for (schema_name, schema) in &components.schemas {
        validate_schema_refs(document, schema, &format!("schema `{schema_name}`"))?;
    }

    for (scheme_name, scheme) in &components.security_schemes {
        document
            .resolve::<OasSecurityScheme>(scheme, &format!("security scheme `{scheme_name}`"))?;
    }

    for (parameter_name, parameter) in &components.parameters {
        validate_parameter_ref(
            document,
            parameter,
            &format!("parameter `{parameter_name}`"),
        )?;
    }

    for (request_body_name, request_body) in &components.request_bodies {
        validate_request_body_ref(
            document,
            request_body,
            &format!("request body `{request_body_name}`"),
        )?;
    }

    for (response_name, response) in &components.responses {
        validate_response_ref(document, response, &format!("response `{response_name}`"))?;
    }

    for (path_item_name, path_item) in &components.path_items {
        validate_path_item_component_ref(
            document,
            path_item,
            &format!("path item `{path_item_name}`"),
        )?;
    }

    Ok(())
}

fn validate_path_refs(document: &ResolvedDocument<'_>) -> Result<(), ValidationError> {
    let Some(paths) = document.spec.paths.as_ref() else {
        return Ok(());
    };

    for (path, path_item) in paths {
        validate_path_item(document, path_item, &format!("path item `{path}`"))?;
    }

    Ok(())
}

fn validate_schema_refs(
    document: &ResolvedDocument<'_>,
    schema: &OasSchema,
    context: &str,
) -> Result<(), ValidationError> {
    document.resolve_schema(schema, context)?;

    for (index, subschema) in schema.subschemas().enumerate() {
        validate_schema_refs(
            document,
            subschema,
            &format!("{context}.subschemas[{index}]"),
        )?;
    }

    Ok(())
}

fn validate_parameter_ref(
    document: &ResolvedDocument<'_>,
    parameter: &ObjectOrReference<OasParameter>,
    context: &str,
) -> Result<(), ValidationError> {
    let parameter = document.resolve(parameter, context)?;
    if let Some(schema) = parameter.schema.as_ref() {
        validate_schema_refs(document, schema, &format!("{context}.schema"))?;
    }
    Ok(())
}

fn validate_request_body_ref(
    document: &ResolvedDocument<'_>,
    request_body: &ObjectOrReference<OasRequestBody>,
    context: &str,
) -> Result<(), ValidationError> {
    let request_body = document.resolve(request_body, context)?;
    validate_content_schema_refs(
        document,
        &request_body.content,
        &format!("{context}.content"),
    )
}

fn validate_response_ref(
    document: &ResolvedDocument<'_>,
    response: &ObjectOrReference<OasResponse>,
    context: &str,
) -> Result<(), ValidationError> {
    let response = document.resolve(response, context)?;
    validate_content_schema_refs(document, &response.content, &format!("{context}.content"))
}

fn validate_path_item_component_ref(
    document: &ResolvedDocument<'_>,
    path_item: &ObjectOrReference<OasPathItem>,
    context: &str,
) -> Result<(), ValidationError> {
    let path_item = document.resolve_path_item_component(path_item, context)?;
    validate_resolved_path_item(document, path_item, context)
}

fn validate_path_item(
    document: &ResolvedDocument<'_>,
    path_item: &OasPathItem,
    context: &str,
) -> Result<(), ValidationError> {
    let path_item = document.resolve_path_item(path_item, context)?;
    validate_resolved_path_item(document, path_item, context)
}

fn validate_resolved_path_item(
    document: &ResolvedDocument<'_>,
    path_item: &OasPathItem,
    context: &str,
) -> Result<(), ValidationError> {
    for parameter in &path_item.parameters {
        validate_parameter_ref(document, parameter, &format!("{context}.parameters"))?;
    }

    for (method, operation) in [
        ("get", path_item.get.as_ref()),
        ("post", path_item.post.as_ref()),
        ("put", path_item.put.as_ref()),
        ("patch", path_item.patch.as_ref()),
        ("delete", path_item.delete.as_ref()),
        ("head", path_item.head.as_ref()),
        ("options", path_item.options.as_ref()),
        ("trace", path_item.trace.as_ref()),
    ] {
        validate_operation_refs(document, operation, &format!("{context}.{method}"))?;
    }

    Ok(())
}

fn validate_operation_refs(
    document: &ResolvedDocument<'_>,
    operation: Option<&OasOperation>,
    context: &str,
) -> Result<(), ValidationError> {
    let Some(operation) = operation else {
        return Ok(());
    };

    let operation_context = operation
        .operation_id
        .as_ref()
        .map(|operation_id| format!("operation `{operation_id}`"))
        .unwrap_or_else(|| context.to_owned());

    for parameter in &operation.parameters {
        validate_parameter_ref(
            document,
            parameter,
            &format!("{operation_context} parameters"),
        )?;
    }

    if let Some(request_body) = operation.request_body.as_ref() {
        validate_request_body_ref(
            document,
            request_body,
            &format!("{operation_context} requestBody"),
        )?;
    }

    if let Some(responses) = operation.responses.as_ref() {
        for (status, response) in responses {
            validate_response_ref(
                document,
                response,
                &format!("{operation_context} responses {status}"),
            )?;
        }
    }

    Ok(())
}

fn validate_content_schema_refs(
    document: &ResolvedDocument<'_>,
    content: &OasMap<String, OasMediaType>,
    location: &str,
) -> Result<(), ValidationError> {
    for (media_type, media) in content {
        if let Some(schema) = media.schema.as_ref() {
            validate_schema_refs(document, schema, &format!("{location}.{media_type}.schema"))?;
        }
    }

    Ok(())
}
