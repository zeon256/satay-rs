use oas3::{
    Map as OasMap,
    spec::{
        ObjectOrReference, Operation as OasOperation, Parameter as OasParameter,
        RequestBody as OasRequestBody, Response as OasResponse,
    },
};

use super::super::helpers::json_media_type;
use super::super::reference::{
    resolve_parameter, resolve_path_item, resolve_request_body, resolve_response,
};
use super::super::resolve::ResolvedDocument;
use super::schema::validate_type_schema;
use crate::error::ValidationError;
use crate::model::HttpMethod;

pub(super) fn validate_operations(document: &ResolvedDocument<'_>) -> Result<(), ValidationError> {
    let Some(paths) = document.spec.paths.as_ref() else {
        return Ok(());
    };

    for (path, path_item) in paths {
        let path_item = resolve_path_item(document, path_item, &format!("path item `{path}`"))?;

        validate_parameter_list(
            document,
            &path_item.parameters,
            &format!("path item `{path}` parameters"),
        )?;

        validate_path_operation(document, HttpMethod::Get, path, path_item.get.as_ref())?;
        validate_path_operation(document, HttpMethod::Post, path, path_item.post.as_ref())?;
        validate_path_operation(document, HttpMethod::Put, path, path_item.put.as_ref())?;
        validate_path_operation(document, HttpMethod::Patch, path, path_item.patch.as_ref())?;
        validate_path_operation(
            document,
            HttpMethod::Delete,
            path,
            path_item.delete.as_ref(),
        )?;
        validate_path_operation(document, HttpMethod::Head, path, path_item.head.as_ref())?;
        validate_path_operation(
            document,
            HttpMethod::Options,
            path,
            path_item.options.as_ref(),
        )?;
        validate_path_operation(document, HttpMethod::Trace, path, path_item.trace.as_ref())?;
    }

    Ok(())
}

fn validate_path_operation(
    document: &ResolvedDocument<'_>,
    method: HttpMethod,
    path: &str,
    operation: Option<&OasOperation>,
) -> Result<(), ValidationError> {
    let Some(operation) = operation else {
        return Ok(());
    };

    validate_operation(document, method, path, operation)
}

fn validate_operation(
    document: &ResolvedDocument<'_>,
    method: HttpMethod,
    path: &str,
    operation: &OasOperation,
) -> Result<(), ValidationError> {
    let operation_id = operation
        .operation_id
        .clone()
        .unwrap_or_else(|| inferred_operation_id(method, path));

    validate_parameter_list(
        document,
        &operation.parameters,
        &format!("operation `{operation_id}` parameters"),
    )?;

    validate_request_body(
        document,
        operation.request_body.as_ref(),
        &format!("operation `{operation_id}` requestBody"),
    )?;

    if let Some(responses) = operation.responses.as_ref() {
        validate_responses(
            document,
            responses,
            &format!("operation `{operation_id}` responses"),
        )?;
    }

    Ok(())
}

fn validate_parameter_list(
    document: &ResolvedDocument<'_>,
    parameters: &[ObjectOrReference<OasParameter>],
    context: &str,
) -> Result<(), ValidationError> {
    for parameter in parameters {
        validate_parameter(document, parameter, context)?;
    }

    Ok(())
}

fn validate_parameter(
    document: &ResolvedDocument<'_>,
    parameter: &ObjectOrReference<OasParameter>,
    context: &str,
) -> Result<(), ValidationError> {
    let parameter = resolve_parameter(document, parameter, context)?;

    if parameter.content.is_some() {
        return Ok(());
    }

    let Some(schema) = parameter.schema.as_ref() else {
        return Ok(());
    };

    validate_type_schema(schema, &format!("parameter `{}`", parameter.name), false)
}

fn validate_request_body(
    document: &ResolvedDocument<'_>,
    request_body: Option<&ObjectOrReference<OasRequestBody>>,
    context: &str,
) -> Result<(), ValidationError> {
    let Some(request_body) = request_body else {
        return Ok(());
    };

    let request_body = resolve_request_body(document, request_body, context)?;
    let Some((_, media_type)) = json_media_type(&request_body.content) else {
        return Ok(());
    };
    let Some(schema) = media_type.schema.as_ref() else {
        return Ok(());
    };

    validate_type_schema(schema, context, false)
}

fn validate_responses(
    document: &ResolvedDocument<'_>,
    responses: &OasMap<String, ObjectOrReference<OasResponse>>,
    context: &str,
) -> Result<(), ValidationError> {
    for (status, response) in responses {
        if status == "default" {
            continue;
        }

        let response = resolve_response(document, response, &format!("{context} {status}"))?;

        if response.content.is_empty() {
            continue;
        }

        let Some((_, media_type)) = json_media_type(&response.content) else {
            continue;
        };
        let Some(schema) = media_type.schema.as_ref() else {
            continue;
        };

        validate_type_schema(schema, &format!("{context} {status} schema"), false)?;
    }

    Ok(())
}

fn inferred_operation_id(method: HttpMethod, path: &str) -> String {
    let mut parts = Vec::new();
    parts.push(method.operation_prefix().to_owned());
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        if let Some(name) = segment
            .strip_prefix('{')
            .and_then(|part| part.strip_suffix('}'))
        {
            parts.push("by".to_owned());
            parts.push(name.to_owned());
        } else {
            parts.push(segment.to_owned());
        }
    }
    parts.join("_")
}
