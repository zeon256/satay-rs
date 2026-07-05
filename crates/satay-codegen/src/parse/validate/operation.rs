use std::collections::BTreeSet;

use oas3::{
    Map as OasMap,
    spec::{
        ObjectOrReference, Operation as OasOperation, Parameter as OasParameter,
        ParameterIn as OasParameterIn, RequestBody as OasRequestBody, Response as OasResponse,
    },
};

use super::super::helpers::{json_media_type, optional_description};
use super::super::reference::{
    resolve_parameter, resolve_path_item, resolve_request_body, resolve_response,
};
use super::super::resolve::ResolvedDocument;
use super::schema::{schema_uses_all_of, schema_uses_any_of, validate_type_schema};
use super::{ValidatedOperation, ValidatedParameter, ValidatedRequestBody, ValidatedResponse};
use crate::error::ValidationError;
use crate::model::{HttpMethod, ParameterLocation, PathSegment};

pub(super) fn validate_operations(
    document: &ResolvedDocument<'_>,
) -> Result<Vec<ValidatedOperation>, ValidationError> {
    let paths = document
        .spec
        .paths
        .as_ref()
        .ok_or(ValidationError::MissingPaths)?;

    let mut operations = vec![];

    for (path, path_item) in paths {
        let path_item = resolve_path_item(document, path_item, &format!("path item `{path}`"))?;

        let path_parameters = validate_parameter_list(
            document,
            &path_item.parameters,
            &format!("path item `{path}` parameters"),
        )?;

        for (method, operation) in [
            (HttpMethod::Get, path_item.get.as_ref()),
            (HttpMethod::Post, path_item.post.as_ref()),
            (HttpMethod::Put, path_item.put.as_ref()),
            (HttpMethod::Patch, path_item.patch.as_ref()),
            (HttpMethod::Delete, path_item.delete.as_ref()),
            (HttpMethod::Head, path_item.head.as_ref()),
            (HttpMethod::Options, path_item.options.as_ref()),
            (HttpMethod::Trace, path_item.trace.as_ref()),
        ] {
            validate_path_operation(
                document,
                &mut operations,
                method,
                path,
                operation,
                &path_parameters,
            )?;
        }
    }

    Ok(operations)
}

fn validate_path_operation(
    document: &ResolvedDocument<'_>,
    operations: &mut Vec<ValidatedOperation>,
    method: HttpMethod,
    path: &str,
    operation: Option<&OasOperation>,
    path_parameters: &[ValidatedParameter],
) -> Result<(), ValidationError> {
    let Some(operation) = operation else {
        return Ok(());
    };

    operations.push(validate_operation(
        document,
        method,
        path,
        path_parameters,
        operation,
    )?);

    Ok(())
}

fn validate_operation(
    document: &ResolvedDocument<'_>,
    method: HttpMethod,
    path: &str,
    path_parameters: &[ValidatedParameter],
    operation: &OasOperation,
) -> Result<ValidatedOperation, ValidationError> {
    let operation_id = operation
        .operation_id
        .clone()
        .unwrap_or_else(|| inferred_operation_id(method, path));

    let mut parameters = path_parameters.to_vec();

    for parameter in validate_parameter_list(
        document,
        &operation.parameters,
        &format!("operation `{operation_id}` parameters"),
    )? {
        upsert_parameter(&mut parameters, parameter);
    }

    validate_path_parameters(path, &parameters)?;

    let request_body = validate_request_body(
        document,
        operation.request_body.as_ref(),
        &format!("operation `{operation_id}` requestBody"),
    )?;

    let Some(responses) = operation.responses.as_ref() else {
        return Err(ValidationError::MissingOperationResponses {
            operation_id: operation_id.clone(),
        });
    };
    let responses = validate_responses(
        document,
        responses,
        &format!("operation `{operation_id}` responses"),
    )?;

    Ok(ValidatedOperation {
        operation_id,
        description: optional_description(&operation.description),
        method,
        path: path.to_owned(),
        path_segments: parse_path_segments(path)?,
        parameters,
        request_body,
        responses,
    })
}

fn validate_parameter_list(
    document: &ResolvedDocument<'_>,
    parameters: &[ObjectOrReference<OasParameter>],
    context: &str,
) -> Result<Vec<ValidatedParameter>, ValidationError> {
    let mut parsed = Vec::with_capacity(parameters.len());

    for parameter in parameters {
        parsed.push(validate_parameter(document, parameter, context)?);
    }

    Ok(parsed)
}

fn validate_parameter(
    document: &ResolvedDocument<'_>,
    parameter: &ObjectOrReference<OasParameter>,
    context: &str,
) -> Result<ValidatedParameter, ValidationError> {
    let parameter = resolve_parameter(document, parameter, context)?;
    let wire_name = parameter.name.clone();

    let location = match parameter.location {
        OasParameterIn::Path => ParameterLocation::Path,
        OasParameterIn::Query => ParameterLocation::Query,
        OasParameterIn::Header => ParameterLocation::Header,
        OasParameterIn::Cookie => {
            return Err(ValidationError::UnsupportedParameterLocation {
                context: context.to_owned(),
                wire_name,
                location: "cookie".to_owned(),
            });
        }
    };

    if parameter.content.is_some() {
        return Err(ValidationError::ContentParameterUnsupported {
            context: context.to_owned(),
            wire_name,
        });
    }

    let schema =
        parameter
            .schema
            .as_ref()
            .ok_or_else(|| ValidationError::MissingParameterSchema {
                context: context.to_owned(),
                wire_name: wire_name.clone(),
            })?;

    if schema_uses_all_of(document, schema)? {
        return Err(ValidationError::UnsupportedComposition {
            context: format!("parameter `{wire_name}`"),
            keyword: "allOf",
        });
    }

    let ty = validate_type_schema(document, schema, &format!("parameter `{wire_name}`"), false)?;

    if ty.is_nullable() {
        return Err(ValidationError::NullableParameterUnsupported {
            wire_name: wire_name.clone(),
        });
    }

    if ty.contains_inline_struct() {
        return Err(ValidationError::UnsupportedComposition {
            context: format!("parameter `{wire_name}`"),
            keyword: "allOf",
        });
    }

    if ty.contains_any_of() || schema_uses_any_of(document, schema)? {
        return Err(ValidationError::AnyOfParameterUnsupported {
            wire_name: wire_name.clone(),
        });
    }

    if ty.contains_map_or_json_value() {
        return Err(ValidationError::MapParameterUnsupported {
            wire_name: wire_name.clone(),
        });
    }

    if location == ParameterLocation::Path && ty.is_array() {
        return Err(ValidationError::ArrayPathParameterUnsupported {
            wire_name: wire_name.clone(),
        });
    }

    if location == ParameterLocation::Header && ty.is_array() {
        return Err(ValidationError::ArrayHeaderParameterUnsupported {
            wire_name: wire_name.clone(),
        });
    }

    let required = match location {
        ParameterLocation::Path => {
            if parameter.required != Some(true) {
                return Err(ValidationError::PathParameterNotRequired { wire_name });
            }
            true
        }
        ParameterLocation::Query | ParameterLocation::Header => parameter.required.unwrap_or(false),
    };

    Ok(ValidatedParameter {
        location,
        wire_name: parameter.name.clone(),
        description: optional_description(&parameter.description),
        ty,
        required,
    })
}

fn validate_request_body(
    document: &ResolvedDocument<'_>,
    request_body: Option<&ObjectOrReference<OasRequestBody>>,
    context: &str,
) -> Result<Option<ValidatedRequestBody>, ValidationError> {
    let Some(request_body) = request_body else {
        return Ok(None);
    };

    let request_body = resolve_request_body(document, request_body, context)?;

    if request_body.content.is_empty() {
        return Err(ValidationError::MissingContent {
            context: context.to_owned(),
        });
    }

    let (content_type, media_type) = json_media_type(&request_body.content).ok_or_else(|| {
        ValidationError::MissingJsonContent {
            context: context.to_owned(),
        }
    })?;
    let schema = media_type
        .schema
        .as_ref()
        .ok_or_else(|| ValidationError::MissingJsonSchema {
            context: context.to_owned(),
        })?;

    Ok(Some(ValidatedRequestBody {
        description: optional_description(&request_body.description),
        content_type: content_type.to_owned(),
        ty: validate_type_schema(document, schema, context, false)?,
        required: request_body.required.unwrap_or(false),
    }))
}

fn validate_responses(
    document: &ResolvedDocument<'_>,
    responses: &OasMap<String, ObjectOrReference<OasResponse>>,
    context: &str,
) -> Result<Vec<ValidatedResponse>, ValidationError> {
    let mut parsed = vec![];

    for (status, response) in responses {
        if status == "default" {
            let response = resolve_response(document, response, &format!("{context} default"))?;
            if !response.content.is_empty() {
                return Err(ValidationError::DefaultResponseBodyUnsupported {
                    context: context.to_owned(),
                });
            }
            continue;
        }

        let status_code =
            status
                .parse::<u16>()
                .map_err(|_| ValidationError::InvalidStatusCode {
                    context: context.to_owned(),
                    status: status.to_owned(),
                })?;
        if !(100..=599).contains(&status_code) {
            return Err(ValidationError::OutOfRangeStatusCode {
                context: context.to_owned(),
                status_code,
            });
        }

        let response = resolve_response(document, response, &format!("{context} {status}"))?;

        let body = if response.content.is_empty() {
            None
        } else {
            let (_, media_type) = json_media_type(&response.content).ok_or_else(|| {
                ValidationError::MissingResponseJsonContent {
                    context: context.to_owned(),
                    status: status.to_owned(),
                }
            })?;
            media_type
                .schema
                .as_ref()
                .map(|schema| {
                    validate_type_schema(
                        document,
                        schema,
                        &format!("{context} {status} schema"),
                        false,
                    )
                })
                .transpose()?
        };

        parsed.push(ValidatedResponse {
            status: status_code,
            description: optional_description(&response.description),
            body,
        });
    }

    parsed.sort_by_key(|response| response.status);
    Ok(parsed)
}

fn upsert_parameter(parameters: &mut Vec<ValidatedParameter>, parameter: ValidatedParameter) {
    if let Some(existing) = parameters.iter_mut().find(|existing| {
        existing.location == parameter.location && existing.wire_name == parameter.wire_name
    }) {
        *existing = parameter;
    } else {
        parameters.push(parameter);
    }
}

fn validate_path_parameters(
    path: &str,
    parameters: &[ValidatedParameter],
) -> Result<(), ValidationError> {
    let declared = parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|parameter| parameter.wire_name.as_str())
        .collect::<BTreeSet<_>>();

    let placeholders = path_parameter_names(path)?;
    for name in &placeholders {
        if !declared.contains(name.as_str()) {
            return Err(ValidationError::UndeclaredPathParameter {
                path: path.to_owned(),
                name: name.clone(),
            });
        }
    }

    for name in declared {
        if !placeholders.contains(name) {
            return Err(ValidationError::UnusedPathParameter {
                path: path.to_owned(),
                name: name.to_owned(),
            });
        }
    }

    Ok(())
}

fn path_parameter_names(path: &str) -> Result<BTreeSet<String>, ValidationError> {
    let mut names = BTreeSet::new();
    let mut rest = path;

    loop {
        let Some(open) = rest.find('{') else {
            return Ok(names);
        };

        let close = rest[open + 1..].find('}').ok_or_else(|| {
            let path = path.to_owned();
            ValidationError::UnclosedPathParameter { path }
        })?;

        names.insert(rest[open + 1..open + 1 + close].to_owned());
        rest = &rest[open + 1 + close + 1..];
    }
}

fn parse_path_segments(path: &str) -> Result<Vec<PathSegment>, ValidationError> {
    let mut segments = vec![];
    let mut rest = path;

    loop {
        let Some(open) = rest.find('{') else {
            if !rest.is_empty() {
                segments.push(PathSegment::Literal(rest.to_owned()));
            }
            return Ok(segments);
        };

        let close = rest[open + 1..].find('}').ok_or_else(|| {
            let path = path.to_owned();
            ValidationError::UnclosedPathParameter { path }
        })?;

        if open > 0 {
            segments.push(PathSegment::Literal(rest[..open].to_owned()));
        }

        segments.push(PathSegment::Parameter(
            rest[open + 1..open + 1 + close].to_owned(),
        ));

        rest = &rest[open + 1 + close + 1..];
    }
}

fn inferred_operation_id(method: HttpMethod, path: &str) -> String {
    let mut parts = vec![];
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
