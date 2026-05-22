use std::collections::BTreeSet;

use oas3::{
    Map as OasMap,
    spec::{
        ObjectOrReference, Operation, Parameter as OasParameter, ParameterIn as OasParameterIn,
        RequestBody as OasRequestBody, Response as OasResponse,
        SecurityScheme as OasSecurityScheme,
    },
};

use super::super::helpers::{json_media_type, optional_description};
use super::super::reference::{
    resolve_parameter, resolve_path_item, resolve_request_body, resolve_response,
    resolve_security_scheme,
};
use super::super::registry::TypeRegistry;
use super::super::resolve::ResolvedDocument;
use super::super::validate::ValidatedDocument;
use super::schema::parse_type_ref;
use crate::error::ValidationError;
use crate::ident::{field_ident, function_ident, response_variant_ident, type_ident, unique_ident};
use crate::model::{
    ApiKeyLocation, ApiKeySecurityScheme, HttpMethod, Operation as SatayOperation, Parameter,
    ParameterLocation, PathSegment, RequestBody, ResponseCase, is_array_type,
};

pub(super) fn parse_api_key_security_schemes(
    document: &ResolvedDocument<'_>,
) -> Result<Vec<ApiKeySecurityScheme>, ValidationError> {
    let Some(components) = document.spec.components.as_ref() else {
        return Ok(vec![]);
    };

    let mut used = BTreeSet::from([
        "apply".to_owned(),
        "base_url".to_owned(),
        "http".to_owned(),
        "new".to_owned(),
    ]);

    let mut schemes = vec![];

    for (scheme_name, scheme) in &components.security_schemes {
        let context = format!("security scheme `{scheme_name}`");
        let scheme = resolve_security_scheme(document, scheme, &context)?;
        let OasSecurityScheme::ApiKey { name, location, .. } = scheme else {
            continue;
        };

        let location = match location.as_str() {
            "header" => ApiKeyLocation::Header,
            "query" => ApiKeyLocation::Query,
            _ => continue,
        };
        let wire_name = name.clone();
        let rust_name = unique_ident(field_ident(&wire_name), &mut used);
        schemes.push(ApiKeySecurityScheme {
            location,
            wire_name,
            rust_name,
        });
    }

    Ok(schemes)
}

pub(super) fn parse_operations(
    document: &ValidatedDocument<'_>,
    registry: &mut TypeRegistry,
) -> Result<Vec<SatayOperation>, ValidationError> {
    let spec = &document.resolved.spec;
    let paths = spec
        .paths
        .as_ref()
        .expect("validation should require OpenAPI paths before lowering");

    let mut operations = vec![];

    for (path, path_item) in paths {
        let path_item = resolve_path_item(
            &document.resolved,
            path_item,
            &format!("path item `{path}`"),
        )?;

        let path_parameter_prefix = type_ident(&format!("{path} parameter"));
        let path_parameters = parse_parameter_list(
            document,
            &path_item.parameters,
            &format!("path item `{path}` parameters"),
            registry,
            &path_parameter_prefix,
        )?;

        parse_path_operation(
            document,
            &mut operations,
            HttpMethod::Get,
            path,
            path_item.get.as_ref(),
            &path_parameters,
            registry,
        )?;

        parse_path_operation(
            document,
            &mut operations,
            HttpMethod::Post,
            path,
            path_item.post.as_ref(),
            &path_parameters,
            registry,
        )?;

        parse_path_operation(
            document,
            &mut operations,
            HttpMethod::Put,
            path,
            path_item.put.as_ref(),
            &path_parameters,
            registry,
        )?;

        parse_path_operation(
            document,
            &mut operations,
            HttpMethod::Patch,
            path,
            path_item.patch.as_ref(),
            &path_parameters,
            registry,
        )?;

        parse_path_operation(
            document,
            &mut operations,
            HttpMethod::Delete,
            path,
            path_item.delete.as_ref(),
            &path_parameters,
            registry,
        )?;

        parse_path_operation(
            document,
            &mut operations,
            HttpMethod::Head,
            path,
            path_item.head.as_ref(),
            &path_parameters,
            registry,
        )?;

        parse_path_operation(
            document,
            &mut operations,
            HttpMethod::Options,
            path,
            path_item.options.as_ref(),
            &path_parameters,
            registry,
        )?;

        parse_path_operation(
            document,
            &mut operations,
            HttpMethod::Trace,
            path,
            path_item.trace.as_ref(),
            &path_parameters,
            registry,
        )?;
    }

    Ok(operations)
}

#[allow(clippy::too_many_arguments)]
fn parse_path_operation(
    document: &ValidatedDocument<'_>,
    operations: &mut Vec<SatayOperation>,
    method: HttpMethod,
    path: &str,
    operation: Option<&Operation>,
    path_parameters: &[Parameter],
    registry: &mut TypeRegistry,
) -> Result<(), ValidationError> {
    let Some(operation) = operation else {
        return Ok(());
    };

    operations.push(parse_operation(
        document,
        method,
        path,
        path_parameters,
        operation,
        registry,
    )?);

    Ok(())
}

fn parse_operation(
    document: &ValidatedDocument<'_>,
    method: HttpMethod,
    path: &str,
    path_parameters: &[Parameter],
    operation: &Operation,
    registry: &mut TypeRegistry,
) -> Result<SatayOperation, ValidationError> {
    let operation_id = operation
        .operation_id
        .clone()
        .unwrap_or_else(|| inferred_operation_id(method, path));

    let description = optional_description(&operation.description);
    let fn_name = function_ident(&operation_id);
    let type_prefix = type_ident(&operation_id);

    let mut parameters = path_parameters.to_vec();

    for parameter in parse_parameter_list(
        document,
        &operation.parameters,
        &format!("operation `{operation_id}` parameters"),
        registry,
        &type_prefix,
    )? {
        upsert_parameter(&mut parameters, parameter);
    }
    deduplicate_parameter_fields(&mut parameters);

    let path_segments = parse_path_segments(path);

    let request_body = parse_request_body(
        document,
        operation.request_body.as_ref(),
        &format!("operation `{operation_id}` requestBody"),
        &parameters,
        registry,
        &type_prefix,
    )?;

    let responses = parse_responses(
        document,
        operation
            .responses
            .as_ref()
            .expect("validation should require operation responses before lowering"),
        &format!("operation `{operation_id}` responses"),
        registry,
        &type_prefix,
    )?;

    Ok(SatayOperation {
        fn_name,
        description,
        input_name: format!("{type_prefix}Input"),
        response_name: format!("{type_prefix}Response"),
        method,
        path: path.to_owned(),
        path_segments,
        parameters,
        request_body,
        responses,
    })
}

fn parse_parameter_list(
    document: &ValidatedDocument<'_>,
    parameters: &[ObjectOrReference<OasParameter>],
    context: &str,
    registry: &mut TypeRegistry,
    type_prefix: &str,
) -> Result<Vec<Parameter>, ValidationError> {
    let mut parsed = Vec::with_capacity(parameters.len());

    for parameter in parameters {
        parsed.push(parse_parameter(
            document,
            parameter,
            context,
            registry,
            type_prefix,
        )?);
    }
    Ok(parsed)
}

fn parse_parameter(
    document: &ValidatedDocument<'_>,
    parameter: &ObjectOrReference<OasParameter>,
    context: &str,
    registry: &mut TypeRegistry,
    type_prefix: &str,
) -> Result<Parameter, ValidationError> {
    let parameter = resolve_parameter(&document.resolved, parameter, context)?;

    let wire_name = parameter.name.clone();

    let location = match parameter.location {
        OasParameterIn::Path => ParameterLocation::Path,
        OasParameterIn::Query => ParameterLocation::Query,
        OasParameterIn::Header => ParameterLocation::Header,
        OasParameterIn::Cookie => {
            unreachable!("validation should reject cookie parameters before lowering");
        }
    };

    if parameter.content.is_some() {
        unreachable!("validation should reject content parameters before lowering");
    }

    let schema = parameter
        .schema
        .as_ref()
        .expect("validation should require parameter schemas before lowering");

    let ty = parse_type_ref(
        schema,
        &format!("parameter `{wire_name}`"),
        registry,
        Some(&format!("{type_prefix} {wire_name} parameter")),
        &document.schemas,
    )?;

    if ty.is_nullable() {
        unreachable!("validation should reject nullable parameters before lowering");
    }

    if location == ParameterLocation::Path && is_array_type(ty.non_nullable()) {
        unreachable!("validation should reject array path parameters before lowering");
    }

    if location == ParameterLocation::Header && is_array_type(ty.non_nullable()) {
        unreachable!("validation should reject array header parameters before lowering");
    }

    let required = match location {
        ParameterLocation::Path => {
            if parameter.required != Some(true) {
                unreachable!("validation should reject optional path parameters before lowering");
            }
            true
        }
        ParameterLocation::Query | ParameterLocation::Header => parameter.required.unwrap_or(false),
    };

    Ok(Parameter {
        location,
        wire_name: wire_name.clone(),
        rust_name: field_ident(&wire_name),
        description: optional_description(&parameter.description),
        ty,
        required,
    })
}

fn upsert_parameter(parameters: &mut Vec<Parameter>, parameter: Parameter) {
    if let Some(existing) = parameters.iter_mut().find(|existing| {
        existing.location == parameter.location && existing.wire_name == parameter.wire_name
    }) {
        *existing = parameter;
    } else {
        parameters.push(parameter);
    }
}

fn deduplicate_parameter_fields(parameters: &mut [Parameter]) {
    let mut used = BTreeSet::new();
    for parameter in parameters {
        parameter.rust_name = unique_ident(parameter.rust_name.clone(), &mut used);
    }
}

fn parse_request_body(
    document: &ValidatedDocument<'_>,
    request_body: Option<&ObjectOrReference<OasRequestBody>>,
    context: &str,
    parameters: &[Parameter],
    registry: &mut TypeRegistry,
    type_prefix: &str,
) -> Result<Option<RequestBody>, ValidationError> {
    let Some(request_body) = request_body else {
        return Ok(None);
    };
    let request_body = resolve_request_body(&document.resolved, request_body, context)?;

    if request_body.content.is_empty() {
        unreachable!("validation should require request body content before lowering");
    }

    let (content_type, media_type) = json_media_type(&request_body.content)
        .expect("validation should require request body JSON content before lowering");

    let schema = media_type
        .schema
        .as_ref()
        .expect("validation should require request body JSON schemas before lowering");

    let mut used = parameters
        .iter()
        .map(|parameter| parameter.rust_name.clone())
        .collect::<BTreeSet<_>>();
    let field_name = unique_ident("body".to_owned(), &mut used);

    Ok(Some(RequestBody {
        field_name,
        description: optional_description(&request_body.description),
        content_type: content_type.to_owned(),
        ty: parse_type_ref(
            schema,
            context,
            registry,
            Some(&format!("{type_prefix} request body")),
            &document.schemas,
        )?,
        required: request_body.required.unwrap_or(false),
    }))
}

fn parse_responses(
    document: &ValidatedDocument<'_>,
    responses: &OasMap<String, ObjectOrReference<OasResponse>>,
    context: &str,
    registry: &mut TypeRegistry,
    type_prefix: &str,
) -> Result<Vec<ResponseCase>, ValidationError> {
    let mut cases = vec![];

    for (status, response) in responses {
        if status == "default" {
            let response =
                resolve_response(&document.resolved, response, &format!("{context} default"))?;
            if !response.content.is_empty() {
                unreachable!("validation should reject default response bodies before lowering");
            }
            continue;
        }

        let status_code = status
            .parse::<u16>()
            .expect("validation should reject invalid status codes before lowering");
        if !(100..=599).contains(&status_code) {
            unreachable!("validation should reject out-of-range status codes before lowering");
        }

        let response =
            resolve_response(&document.resolved, response, &format!("{context} {status}"))?;

        let body = if response.content.is_empty() {
            None
        } else {
            let (_, media_type) = json_media_type(&response.content)
                .expect("validation should require response JSON content before lowering");
            match media_type.schema.as_ref() {
                Some(schema) => Some(parse_type_ref(
                    schema,
                    &format!("{context} {status} schema"),
                    registry,
                    Some(&format!("{type_prefix} response {status}")),
                    &document.schemas,
                )?),
                None => None,
            }
        };

        cases.push(ResponseCase {
            status: status_code,
            variant_name: response_variant_ident(status_code),
            description: optional_description(&response.description),
            body,
        });
    }

    cases.sort_by_key(|case| case.status);
    Ok(cases)
}

fn parse_path_segments(path: &str) -> Vec<PathSegment> {
    let mut segments = vec![];
    let mut rest = path;

    loop {
        let Some(open) = rest.find('{') else {
            if !rest.is_empty() {
                segments.push(PathSegment::Literal(rest.to_owned()));
            }
            return segments;
        };

        let close = rest[open + 1..]
            .find('}')
            .expect("validation should reject unclosed path parameters before lowering");

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
