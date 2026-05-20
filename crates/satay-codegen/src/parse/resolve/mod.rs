use std::collections::BTreeSet;

use oas3::Map as OasMap;
use oas3::spec::{
    Components as OasComponents, MediaType as OasMediaType, ObjectOrReference,
    ObjectSchema as OasObjectSchema, Operation as OasOperation, Parameter as OasParameter,
    PathItem as OasPathItem, RequestBody as OasRequestBody, Response as OasResponse,
    Schema as OasSchema, SecurityScheme as OasSecurityScheme, Spec as OasSpec,
};

use super::Document;
use crate::error::ValidationError;

pub(crate) mod refs;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedDocument<'a> {
    pub(crate) spec: &'a OasSpec,
}

pub(crate) fn resolve_document(
    document: &Document,
) -> Result<ResolvedDocument<'_>, ValidationError> {
    let resolved = ResolvedDocument {
        spec: &document.spec,
    };

    validate_component_refs(&resolved)?;
    validate_path_refs(&resolved)?;

    Ok(resolved)
}

fn validate_component_refs(document: &ResolvedDocument<'_>) -> Result<(), ValidationError> {
    let Some(components) = document.spec.components.as_ref() else {
        return Ok(());
    };

    for (schema_name, schema) in &components.schemas {
        validate_schema_refs(document, schema, &format!("schema `{schema_name}`"))?;
    }

    for (scheme_name, scheme) in &components.security_schemes {
        let mut visited = BTreeSet::new();
        validate_security_scheme_ref(
            document,
            scheme,
            &format!("security scheme `{scheme_name}`"),
            &mut visited,
        )?;
    }

    for (parameter_name, parameter) in &components.parameters {
        let mut visited = BTreeSet::new();
        validate_parameter_ref(
            document,
            parameter,
            &format!("parameter `{parameter_name}`"),
            &mut visited,
        )?;
    }

    for (request_body_name, request_body) in &components.request_bodies {
        let mut visited = BTreeSet::new();
        validate_request_body_ref(
            document,
            request_body,
            &format!("request body `{request_body_name}`"),
            &mut visited,
        )?;
    }

    for (response_name, response) in &components.responses {
        let mut visited = BTreeSet::new();
        validate_response_ref(
            document,
            response,
            &format!("response `{response_name}`"),
            &mut visited,
        )?;
    }

    for (path_item_name, path_item) in &components.path_items {
        let mut visited = BTreeSet::new();
        validate_path_item_component_ref(
            document,
            path_item,
            &format!("path item `{path_item_name}`"),
            &mut visited,
        )?;
    }

    Ok(())
}

fn validate_path_refs(document: &ResolvedDocument<'_>) -> Result<(), ValidationError> {
    let Some(paths) = document.spec.paths.as_ref() else {
        return Ok(());
    };

    for (path, path_item) in paths {
        let mut visited = BTreeSet::new();
        validate_path_item(
            document,
            path_item,
            &format!("path item `{path}`"),
            &mut visited,
        )?;
    }

    Ok(())
}

fn validate_schema_refs(
    document: &ResolvedDocument<'_>,
    schema: &OasSchema,
    context: &str,
) -> Result<(), ValidationError> {
    match schema {
        OasSchema::Boolean(_) => Ok(()),
        OasSchema::Object(schema) => match schema.as_ref() {
            ObjectOrReference::Object(schema) => {
                validate_object_schema_refs(document, schema, context)
            }
            ObjectOrReference::Ref { ref_path, .. } => {
                validate_schema_ref(document, ref_path, context)
            }
        },
    }
}

fn validate_object_schema_refs(
    document: &ResolvedDocument<'_>,
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    for (property_name, property_schema) in &schema.properties {
        validate_schema_refs(
            document,
            property_schema,
            &format!("{context}.properties.{property_name}"),
        )?;
    }

    if let Some(items) = schema.items.as_deref() {
        validate_schema_refs(document, items, &format!("{context}.items"))?;
    }

    for (keyword, schemas) in [
        ("oneOf", &schema.one_of),
        ("anyOf", &schema.any_of),
        ("allOf", &schema.all_of),
    ] {
        for (index, schema) in schemas.iter().enumerate() {
            validate_schema_refs(document, schema, &format!("{context}.{keyword}[{index}]"))?;
        }
    }

    Ok(())
}

fn validate_schema_ref(
    document: &ResolvedDocument<'_>,
    reference: &str,
    context: &str,
) -> Result<(), ValidationError> {
    let name = refs::local_ref_name(reference, "schemas").map_err(|source| {
        ValidationError::ResolveReference {
            reference: reference.to_owned(),
            context: context.to_owned(),
            source: Box::new(source),
        }
    })?;

    if document
        .spec
        .components
        .as_ref()
        .and_then(|components| components.schemas.get(&name))
        .is_none()
    {
        return Err(ValidationError::ResolveReference {
            reference: reference.to_owned(),
            context: context.to_owned(),
            source: Box::new(ValidationError::MissingJsonPointerToken { token: name }),
        });
    }

    Ok(())
}

fn validate_security_scheme_ref(
    document: &ResolvedDocument<'_>,
    scheme: &ObjectOrReference<OasSecurityScheme>,
    context: &str,
    visited: &mut BTreeSet<String>,
) -> Result<(), ValidationError> {
    match scheme {
        ObjectOrReference::Object(_) => Ok(()),
        ObjectOrReference::Ref { ref_path, .. } => validate_component_object_ref(
            document,
            ref_path,
            context,
            "securitySchemes",
            visited,
            |components, name| components.security_schemes.get(name),
            validate_security_scheme_ref,
        ),
    }
}

fn validate_parameter_ref(
    document: &ResolvedDocument<'_>,
    parameter: &ObjectOrReference<OasParameter>,
    context: &str,
    visited: &mut BTreeSet<String>,
) -> Result<(), ValidationError> {
    match parameter {
        ObjectOrReference::Object(parameter) => {
            if let Some(schema) = parameter.schema.as_ref() {
                validate_schema_refs(document, schema, &format!("{context}.schema"))?;
            }
            Ok(())
        }
        ObjectOrReference::Ref { ref_path, .. } => validate_component_object_ref(
            document,
            ref_path,
            context,
            "parameters",
            visited,
            |components, name| components.parameters.get(name),
            validate_parameter_ref,
        ),
    }
}

fn validate_request_body_ref(
    document: &ResolvedDocument<'_>,
    request_body: &ObjectOrReference<OasRequestBody>,
    context: &str,
    visited: &mut BTreeSet<String>,
) -> Result<(), ValidationError> {
    match request_body {
        ObjectOrReference::Object(request_body) => validate_content_schema_refs(
            document,
            &request_body.content,
            &format!("{context}.content"),
        ),
        ObjectOrReference::Ref { ref_path, .. } => validate_component_object_ref(
            document,
            ref_path,
            context,
            "requestBodies",
            visited,
            |components, name| components.request_bodies.get(name),
            validate_request_body_ref,
        ),
    }
}

fn validate_response_ref(
    document: &ResolvedDocument<'_>,
    response: &ObjectOrReference<OasResponse>,
    context: &str,
    visited: &mut BTreeSet<String>,
) -> Result<(), ValidationError> {
    match response {
        ObjectOrReference::Object(response) => {
            validate_content_schema_refs(document, &response.content, &format!("{context}.content"))
        }
        ObjectOrReference::Ref { ref_path, .. } => validate_component_object_ref(
            document,
            ref_path,
            context,
            "responses",
            visited,
            |components, name| components.responses.get(name),
            validate_response_ref,
        ),
    }
}

fn validate_path_item_component_ref(
    document: &ResolvedDocument<'_>,
    path_item: &ObjectOrReference<OasPathItem>,
    context: &str,
    visited: &mut BTreeSet<String>,
) -> Result<(), ValidationError> {
    match path_item {
        ObjectOrReference::Object(path_item) => {
            validate_path_item(document, path_item, context, visited)
        }
        ObjectOrReference::Ref { ref_path, .. } => {
            validate_path_item_ref(document, ref_path, context, visited)
        }
    }
}

fn validate_path_item(
    document: &ResolvedDocument<'_>,
    path_item: &OasPathItem,
    context: &str,
    visited: &mut BTreeSet<String>,
) -> Result<(), ValidationError> {
    if let Some(reference) = path_item.reference.as_deref() {
        return validate_path_item_ref(document, reference, context, visited);
    }

    for parameter in &path_item.parameters {
        let mut parameter_visited = BTreeSet::new();
        validate_parameter_ref(
            document,
            parameter,
            &format!("{context}.parameters"),
            &mut parameter_visited,
        )?;
    }

    validate_operation_refs(document, path_item.get.as_ref(), &format!("{context}.get"))?;
    validate_operation_refs(
        document,
        path_item.post.as_ref(),
        &format!("{context}.post"),
    )?;
    validate_operation_refs(document, path_item.put.as_ref(), &format!("{context}.put"))?;
    validate_operation_refs(
        document,
        path_item.patch.as_ref(),
        &format!("{context}.patch"),
    )?;
    validate_operation_refs(
        document,
        path_item.delete.as_ref(),
        &format!("{context}.delete"),
    )?;
    validate_operation_refs(
        document,
        path_item.head.as_ref(),
        &format!("{context}.head"),
    )?;
    validate_operation_refs(
        document,
        path_item.options.as_ref(),
        &format!("{context}.options"),
    )?;
    validate_operation_refs(
        document,
        path_item.trace.as_ref(),
        &format!("{context}.trace"),
    )?;

    Ok(())
}

fn validate_path_item_ref(
    document: &ResolvedDocument<'_>,
    reference: &str,
    context: &str,
    visited: &mut BTreeSet<String>,
) -> Result<(), ValidationError> {
    validate_component_object_ref(
        document,
        reference,
        context,
        "pathItems",
        visited,
        |components, name| components.path_items.get(name),
        validate_path_item_component_ref,
    )
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
        let mut visited = BTreeSet::new();
        validate_parameter_ref(
            document,
            parameter,
            &format!("{operation_context} parameters"),
            &mut visited,
        )?;
    }

    if let Some(request_body) = operation.request_body.as_ref() {
        let mut visited = BTreeSet::new();
        validate_request_body_ref(
            document,
            request_body,
            &format!("{operation_context} requestBody"),
            &mut visited,
        )?;
    }

    if let Some(responses) = operation.responses.as_ref() {
        for (status, response) in responses {
            let mut visited = BTreeSet::new();
            validate_response_ref(
                document,
                response,
                &format!("{operation_context} responses {status}"),
                &mut visited,
            )?;
        }
    }

    Ok(())
}

fn validate_content_schema_refs(
    document: &ResolvedDocument<'_>,
    content: &OasMap<String, OasMediaType>,
    context: &str,
) -> Result<(), ValidationError> {
    for (media_type, media) in content {
        if let Some(schema) = media.schema.as_ref() {
            validate_schema_refs(document, schema, &format!("{context}.{media_type}.schema"))?;
        }
    }

    Ok(())
}

fn validate_component_object_ref<'a, T: 'a, Get, Validate>(
    document: &'a ResolvedDocument<'a>,
    reference: &str,
    context: &str,
    section: &'static str,
    visited: &mut BTreeSet<String>,
    get: Get,
    validate: Validate,
) -> Result<(), ValidationError>
where
    Get: Fn(&'a OasComponents, &str) -> Option<&'a ObjectOrReference<T>>,
    Validate: Fn(
        &'a ResolvedDocument<'a>,
        &'a ObjectOrReference<T>,
        &str,
        &mut BTreeSet<String>,
    ) -> Result<(), ValidationError>,
{
    let name = refs::local_ref_name(reference, section).map_err(|source| {
        ValidationError::ResolveReference {
            reference: reference.to_owned(),
            context: context.to_owned(),
            source: Box::new(source),
        }
    })?;

    if !visited.insert(reference.to_owned()) {
        return Err(ValidationError::ResolveReference {
            reference: reference.to_owned(),
            context: context.to_owned(),
            source: Box::new(ValidationError::CircularReference {
                reference: reference.to_owned(),
            }),
        });
    }

    let target = document
        .spec
        .components
        .as_ref()
        .and_then(|components| get(components, &name))
        .ok_or_else(|| ValidationError::ResolveReference {
            reference: reference.to_owned(),
            context: context.to_owned(),
            source: Box::new(ValidationError::MissingJsonPointerToken { token: name }),
        })?;

    let result = validate(document, target, context, visited);
    visited.remove(reference);
    result
}
