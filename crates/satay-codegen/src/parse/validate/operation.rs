use std::collections::BTreeSet;

use oas3::{
    Map as OasMap,
    spec::{
        ObjectOrReference, Operation as OasOperation, Parameter as OasParameter,
        ParameterIn as OasParameterIn, RequestBody as OasRequestBody, Response as OasResponse,
        Schema as OasSchema, SchemaType as OasSchemaType,
    },
};

use super::super::helpers::json_media_type;
use super::super::normalize::NormalizedDocument;
use super::super::reference::{
    object_schema, resolve_parameter, resolve_path_item, resolve_request_body, resolve_response,
    schema_ref, schema_type_and_nullable,
};
use super::schema::validate_type_schema;
use super::state::ValidatedSchemas;
use crate::error::ValidationError;
use crate::model::{HttpMethod, ParameterLocation};

#[derive(Debug, Clone)]
struct ValidatedParameter {
    location: ParameterLocation,
    wire_name: String,
}

pub(super) fn validate_operations(
    document: &NormalizedDocument<'_>,
    schemas: &mut ValidatedSchemas,
) -> Result<(), ValidationError> {
    let paths = document
        .resolved
        .spec
        .paths
        .as_ref()
        .ok_or(ValidationError::MissingPaths)?;

    for (path, path_item) in paths {
        let path_item = resolve_path_item(
            &document.resolved,
            path_item,
            &format!("path item `{path}`"),
        )?;

        let path_parameters = validate_parameter_list(
            document,
            &path_item.parameters,
            &format!("path item `{path}` parameters"),
            schemas,
        )?;

        validate_path_operation(
            document,
            HttpMethod::Get,
            path,
            path_item.get.as_ref(),
            &path_parameters,
            schemas,
        )?;
        validate_path_operation(
            document,
            HttpMethod::Post,
            path,
            path_item.post.as_ref(),
            &path_parameters,
            schemas,
        )?;
        validate_path_operation(
            document,
            HttpMethod::Put,
            path,
            path_item.put.as_ref(),
            &path_parameters,
            schemas,
        )?;
        validate_path_operation(
            document,
            HttpMethod::Patch,
            path,
            path_item.patch.as_ref(),
            &path_parameters,
            schemas,
        )?;
        validate_path_operation(
            document,
            HttpMethod::Delete,
            path,
            path_item.delete.as_ref(),
            &path_parameters,
            schemas,
        )?;
        validate_path_operation(
            document,
            HttpMethod::Head,
            path,
            path_item.head.as_ref(),
            &path_parameters,
            schemas,
        )?;
        validate_path_operation(
            document,
            HttpMethod::Options,
            path,
            path_item.options.as_ref(),
            &path_parameters,
            schemas,
        )?;
        validate_path_operation(
            document,
            HttpMethod::Trace,
            path,
            path_item.trace.as_ref(),
            &path_parameters,
            schemas,
        )?;
    }

    Ok(())
}

fn validate_path_operation(
    document: &NormalizedDocument<'_>,
    method: HttpMethod,
    path: &str,
    operation: Option<&OasOperation>,
    path_parameters: &[ValidatedParameter],
    schemas: &mut ValidatedSchemas,
) -> Result<(), ValidationError> {
    let Some(operation) = operation else {
        return Ok(());
    };

    validate_operation(document, method, path, path_parameters, operation, schemas)
}

fn validate_operation(
    document: &NormalizedDocument<'_>,
    method: HttpMethod,
    path: &str,
    path_parameters: &[ValidatedParameter],
    operation: &OasOperation,
    schemas: &mut ValidatedSchemas,
) -> Result<(), ValidationError> {
    let operation_id = operation
        .operation_id
        .clone()
        .unwrap_or_else(|| inferred_operation_id(method, path));

    let mut parameters = path_parameters.to_vec();

    for parameter in validate_parameter_list(
        document,
        &operation.parameters,
        &format!("operation `{operation_id}` parameters"),
        schemas,
    )? {
        upsert_parameter(&mut parameters, parameter);
    }

    validate_path_parameters(path, &parameters)?;

    validate_request_body(
        document,
        operation.request_body.as_ref(),
        &format!("operation `{operation_id}` requestBody"),
        schemas,
    )?;

    if let Some(responses) = operation.responses.as_ref() {
        validate_responses(
            document,
            responses,
            &format!("operation `{operation_id}` responses"),
            schemas,
        )?;
    } else {
        return Err(ValidationError::MissingOperationResponses {
            operation_id: operation_id.clone(),
        });
    }

    Ok(())
}

fn validate_parameter_list(
    document: &NormalizedDocument<'_>,
    parameters: &[ObjectOrReference<OasParameter>],
    context: &str,
    schemas: &mut ValidatedSchemas,
) -> Result<Vec<ValidatedParameter>, ValidationError> {
    let mut parsed = Vec::with_capacity(parameters.len());

    for parameter in parameters {
        parsed.push(validate_parameter(document, parameter, context, schemas)?);
    }

    Ok(parsed)
}

fn validate_parameter(
    document: &NormalizedDocument<'_>,
    parameter: &ObjectOrReference<OasParameter>,
    context: &str,
    schemas: &mut ValidatedSchemas,
) -> Result<ValidatedParameter, ValidationError> {
    let parameter = resolve_parameter(&document.resolved, parameter, context)?;
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

    validate_type_schema(
        document,
        schema,
        &format!("parameter `{wire_name}`"),
        false,
        schemas,
    )?;
    validate_parameter_schema_shape(schema, &wire_name, location)?;

    if location == ParameterLocation::Path && parameter.required != Some(true) {
        return Err(ValidationError::PathParameterNotRequired { wire_name });
    }

    Ok(ValidatedParameter {
        location,
        wire_name,
    })
}

fn validate_request_body(
    document: &NormalizedDocument<'_>,
    request_body: Option<&ObjectOrReference<OasRequestBody>>,
    context: &str,
    schemas: &mut ValidatedSchemas,
) -> Result<(), ValidationError> {
    let Some(request_body) = request_body else {
        return Ok(());
    };

    let request_body = resolve_request_body(&document.resolved, request_body, context)?;

    if request_body.content.is_empty() {
        return Err(ValidationError::MissingContent {
            context: context.to_owned(),
        });
    }

    let (_, media_type) = json_media_type(&request_body.content).ok_or_else(|| {
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

    validate_type_schema(document, schema, context, false, schemas)
}

fn validate_responses(
    document: &NormalizedDocument<'_>,
    responses: &OasMap<String, ObjectOrReference<OasResponse>>,
    context: &str,
    schemas: &mut ValidatedSchemas,
) -> Result<(), ValidationError> {
    for (status, response) in responses {
        if status == "default" {
            let response =
                resolve_response(&document.resolved, response, &format!("{context} default"))?;
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

        let response =
            resolve_response(&document.resolved, response, &format!("{context} {status}"))?;

        if response.content.is_empty() {
            continue;
        }

        let (_, media_type) = json_media_type(&response.content).ok_or_else(|| {
            ValidationError::MissingResponseJsonContent {
                context: context.to_owned(),
                status: status.to_owned(),
            }
        })?;
        let Some(schema) = media_type.schema.as_ref() else {
            continue;
        };

        validate_type_schema(
            document,
            schema,
            &format!("{context} {status} schema"),
            false,
            schemas,
        )?;
    }

    Ok(())
}

fn validate_parameter_schema_shape(
    schema: &OasSchema,
    wire_name: &str,
    location: ParameterLocation,
) -> Result<(), ValidationError> {
    let context = format!("parameter `{wire_name}`");
    if schema_ref(schema, &context)?.is_some() {
        return Ok(());
    }

    let schema = object_schema(schema, &context)?;
    let (schema_type, nullable) = schema_type_and_nullable(schema, &context)?;

    if nullable {
        return Err(ValidationError::NullableParameterUnsupported {
            wire_name: wire_name.to_owned(),
        });
    }

    if location == ParameterLocation::Path && schema_type == Some(OasSchemaType::Array) {
        return Err(ValidationError::ArrayPathParameterUnsupported {
            wire_name: wire_name.to_owned(),
        });
    }

    if location == ParameterLocation::Header && schema_type == Some(OasSchemaType::Array) {
        return Err(ValidationError::ArrayHeaderParameterUnsupported {
            wire_name: wire_name.to_owned(),
        });
    }

    Ok(())
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
