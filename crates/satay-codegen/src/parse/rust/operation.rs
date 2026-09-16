//! HTTP lowering over semantic records, preserving encounter and response order.
use super::LowerError;
use super::{
    constraint, policy,
    schema::{Schemas, constraints},
};
use crate::ValidationError;
use crate::ident::{field_ident, unique_ident};
use crate::model::{
    ApiKeyLocation, ApiKeySecurityScheme, HttpMethod, ParameterLocation, PathSegment,
    ResponseStatus, TypeRef,
};
use crate::parse::helpers;
use crate::parse::helpers::is_json_media_type;
use crate::parse::validate::*;
use satay_ir::{
    self as ir, ApiKeyLocation as SemanticApiKeyLocation, CompositionKind,
    HttpMethod as SemanticHttpMethod, ParameterLocation as SemanticParameterLocation,
    ResponseStatus as SemanticResponseStatus, SecuritySchemeKind, TypeExpr,
};
use std::collections::BTreeSet;

pub(super) fn security_schemes(api: &ir::Api) -> Vec<ApiKeySecurityScheme> {
    let mut used = ["apply", "base_url", "string_storage", "http", "new"]
        .map(str::to_owned)
        .into_iter()
        .collect();
    api.http()
        .security_schemes
        .iter()
        .filter_map(|scheme| {
            let SecuritySchemeKind::ApiKey {
                wire_name,
                location,
            } = &scheme.kind
            else {
                return None;
            };
            let location = match location {
                SemanticApiKeyLocation::Header => ApiKeyLocation::Header,
                SemanticApiKeyLocation::Query => ApiKeyLocation::Query,
                SemanticApiKeyLocation::Cookie | SemanticApiKeyLocation::Unsupported(_) => {
                    return None;
                }
            };
            Some(ApiKeySecurityScheme {
                location,
                wire_name: wire_name.clone(),
                rust_name: unique_ident(field_ident(wire_name), &mut used),
            })
        })
        .collect()
}

#[allow(clippy::too_many_lines)] // Validation order is part of the diagnostic contract.
pub(super) fn operations(
    api: &ir::Api,
    schemas: &mut Schemas<'_>,
) -> Result<Vec<ValidatedOperation>, LowerError> {
    if let Some(diagnostic) = &api.http().diagnostic {
        return Err(diagnostic.clone().into());
    }
    let mut output = vec![];
    for path in &api.http().paths {
        if !path.operations.is_empty()
            && path
                .operations
                .iter()
                .all(|operation| operation.interpretation.skip)
        {
            continue;
        }
        let parameters = path
            .parameters
            .iter()
            .map(|parameter| parameter_type(parameter, schemas))
            .collect::<Result<Vec<_>, _>>()?;
        for operation in path
            .operations
            .iter()
            .filter(|operation| !operation.interpretation.skip)
        {
            let method = method(operation.method);
            let operation_id = operation
                .source_id
                .clone()
                .unwrap_or_else(|| inferred_operation_id(method, &path.path));
            let context = format!("operation `{operation_id}`");
            let mut parameters = parameters.clone();
            for parameter in &operation.parameters {
                upsert_parameter(&mut parameters, parameter_type(parameter, schemas)?);
            }
            validate_path_parameters(&path.path, &parameters)?;
            let request_body = operation
                .request_body
                .as_ref()
                .map(|body| -> Result<_, LowerError> {
                    let context = format!("{context} requestBody");
                    if body.content.is_empty() {
                        return Err(ValidationError::MissingContent { context }.into());
                    }
                    let media = select_media(&body.content, |media| media.media_type.as_str())
                        .ok_or_else(|| ValidationError::MissingJsonContent {
                            context: context.clone(),
                        })?;
                    let schema = media.schema.as_ref().ok_or_else(|| {
                        ValidationError::MissingJsonSchema {
                            context: context.clone(),
                        }
                    })?;
                    Ok(ValidatedRequestBody {
                        description: body.description.clone(),
                        content_type: media.media_type.clone(),
                        ty: schemas.value(schema, &context)?,
                        required: body.required,
                    })
                })
                .transpose()?;
            if let Some(diagnostic) = &operation.responses_diagnostic {
                return Err(diagnostic.clone().into());
            }
            let mut responses = vec![];
            for response in &operation.responses {
                let status = match &response.status {
                    SemanticResponseStatus::Invalid(status) => {
                        return Err(ValidationError::InvalidStatusCode {
                            context: format!("{context} responses"),
                            status: status.clone(),
                        }
                        .into());
                    }
                    SemanticResponseStatus::Default => {
                        if !response.content.is_empty() {
                            return Err(ValidationError::DefaultResponseBodyUnsupported {
                                context: format!("{context} responses"),
                            }
                            .into());
                        }
                        continue;
                    }
                    SemanticResponseStatus::Exact(code) => {
                        if !(100..=599).contains(code) {
                            return Err(ValidationError::OutOfRangeStatusCode {
                                context: format!("{context} responses"),
                                status_code: *code,
                            }
                            .into());
                        }
                        ResponseStatus::Exact(*code)
                    }
                    SemanticResponseStatus::Range(class) => ResponseStatus::Range(*class),
                };
                let (body, projection) = if response.content.is_empty() {
                    (None, None)
                } else {
                    let media =
                        select_media(&response.content, |media| media.media.media_type.as_str())
                            .ok_or_else(|| ValidationError::MissingResponseJsonContent {
                                context: format!("{context} responses"),
                                status: status.to_string(),
                            })?;
                    let context = format!("{context} responses {status} schema");
                    match &media.projection {
                        Some(projection) => (
                            Some(schemas.projected_value(projection, &context)?),
                            Some(ValidatedResponseProjection {
                                unwrap_field: projection.selector.unwrap_field.clone(),
                                map_field: projection.selector.map_field.clone(),
                            }),
                        ),
                        None => (
                            media
                                .media
                                .schema
                                .as_ref()
                                .map(|schema| schemas.value(schema, &context))
                                .transpose()?,
                            None,
                        ),
                    }
                };
                responses.push(ValidatedResponse {
                    status,
                    description: response.description.clone(),
                    body,
                    projection,
                });
            }
            responses.sort_by_key(|response| response.status);
            if operation.interpretation.output.is_some()
                && responses.iter().all(|response| response.body.is_none())
            {
                return Err(
                    ValidationError::SatayOutputRequiresResponseBody { operation_id }.into(),
                );
            }
            output.push(ValidatedOperation {
                operation_id,
                tags: operation.tags.clone(),
                description: operation.description.clone(),
                method,
                path: path.path.clone(),
                path_segments: parse_path_segments(&path.path)?,
                parameters,
                request_body,
                responses,
            });
        }
    }
    Ok(output)
}

fn select_media<T>(content: &[T], name: impl Fn(&T) -> &str) -> Option<&T> {
    content
        .iter()
        .find(|entry| name(entry) == "application/json")
        .or_else(|| content.iter().find(|entry| is_json_media_type(name(entry))))
}

#[allow(clippy::too_many_lines)] // Validation order is part of the diagnostic contract.
fn parameter_type(
    parameter: &ir::Parameter,
    schemas: &mut Schemas<'_>,
) -> Result<ValidatedParameter, LowerError> {
    if let TypeExpr::Invalid(diagnostic) = &parameter.schema.ty {
        return Err(diagnostic.clone().into());
    }
    let wire_name = &parameter.wire_name;
    let context = format!("parameter `{wire_name}`");
    let location = match parameter.location {
        SemanticParameterLocation::Path => ParameterLocation::Path,
        SemanticParameterLocation::Header => ParameterLocation::Header,
        SemanticParameterLocation::Query => ParameterLocation::Query,
        SemanticParameterLocation::Cookie => {
            return Err(ValidationError::UnsupportedParameterLocation {
                context,
                wire_name: wire_name.clone(),
                location: "cookie".to_owned(),
            }
            .into());
        }
    };
    let mut schema = &parameter.schema;
    if !parameter.required
        && location != ParameterLocation::Path
        && let TypeExpr::Composition(composition) = &schema.ty
        && composition.kind != CompositionKind::AllOf
        && composition.branches.len() == 2
    {
        for (null, other) in [(0, 1), (1, 0)] {
            if matches!(composition.branches[null].ty, TypeExpr::Null)
                && !matches!(composition.branches[other].ty, TypeExpr::Null)
            {
                schema = &composition.branches[other];
                break;
            }
        }
    }
    if uses_composition(schemas, schema, true, &mut vec![]) {
        return Err(ValidationError::UnsupportedComposition {
            context,
            keyword: "allOf",
        }
        .into());
    }
    let mut ty = schemas.value(schema, &context)?;
    if ty.nullable {
        if !parameter.required && location != ParameterLocation::Path {
            ty.nullable = false;
        } else {
            return Err(ValidationError::NullableParameterUnsupported {
                wire_name: wire_name.clone(),
            }
            .into());
        }
    }
    if ty.contains_inline_struct() {
        return Err(ValidationError::UnsupportedComposition {
            context,
            keyword: "allOf",
        }
        .into());
    }
    if ty.contains_any_of() || uses_composition(schemas, schema, false, &mut vec![]) {
        return Err(ValidationError::AnyOfParameterUnsupported {
            wire_name: wire_name.clone(),
        }
        .into());
    }
    if ty.contains_map_or_json_value() {
        return Err(ValidationError::MapParameterUnsupported {
            wire_name: wire_name.clone(),
        }
        .into());
    }
    if ty.is_array() && location == ParameterLocation::Path {
        return Err(ValidationError::ArrayPathParameterUnsupported {
            wire_name: wire_name.clone(),
        }
        .into());
    }
    if ty.is_array() && location == ParameterLocation::Header {
        return Err(ValidationError::ArrayHeaderParameterUnsupported {
            wire_name: wire_name.clone(),
        }
        .into());
    }
    let resolved = resolve_use(schemas, schema);
    let default_value = parameter
        .schema
        .annotations
        .default
        .as_ref()
        .or(schema.annotations.default.as_ref())
        .or(resolved.annotations.default.as_ref());
    let default = if parameter.required {
        None
    } else if let Some(value) = default_value.filter(|value| !value.is_null()) {
        let resolved_ty = if matches!(ty.kind, ValidatedTypeKind::Named(_)) {
            schemas.value(resolved, &context)?
        } else {
            ty.clone()
        };
        let default = policy::parse_parameter_default(value, &resolved_ty, wire_name)?;
        let enum_validation = if matches!(resolved_ty.kind, ValidatedTypeKind::Enum(_)) {
            constraint::parse_validation(&constraints(resolved), &TypeRef::String, &context)?
        } else {
            None
        };
        policy::validate_parameter_default_constraints(
            &default,
            resolved_ty.validation.as_ref().or(enum_validation.as_ref()),
        )
        .map_err(|reason| policy::invalid_parameter_default(wire_name, value, reason))?;
        Some(default)
    } else {
        // The legacy typed parser erases explicit JSON null defaults. Keep this
        // compatibility policy in Rust; semantic IR still records their presence.
        None
    };
    Ok(ValidatedParameter {
        location,
        wire_name: wire_name.clone(),
        description: parameter.description.clone(),
        ty,
        required: parameter.required,
        default,
    })
}

fn resolve_use<'a>(schemas: &Schemas<'a>, mut value: &'a ir::SchemaUse) -> &'a ir::SchemaUse {
    let mut seen = vec![];
    while let TypeExpr::Ref(id) = value.ty {
        if seen.contains(&id) {
            break;
        }
        seen.push(id);
        value = &schemas.definition(id).schema;
    }
    value
}

fn uses_composition(
    schemas: &Schemas<'_>,
    value: &ir::SchemaUse,
    all_of: bool,
    seen: &mut Vec<ir::DefinitionId>,
) -> bool {
    match &value.ty {
        TypeExpr::Ref(id) => {
            if seen.contains(id) {
                return false;
            }
            seen.push(*id);
            uses_composition(schemas, &schemas.definition(*id).schema, all_of, seen)
        }
        TypeExpr::Composition(composition) => {
            (composition.kind == CompositionKind::AllOf) == all_of
        }
        TypeExpr::Array(array) => uses_composition(schemas, &array.items, all_of, seen),
        TypeExpr::Object(object) if all_of => object
            .properties
            .iter()
            .any(|property| uses_composition(schemas, &property.value, all_of, seen)),
        _ => false,
    }
}

fn method(method: ir::HttpMethod) -> HttpMethod {
    match method {
        SemanticHttpMethod::Get => HttpMethod::Get,
        SemanticHttpMethod::Post => HttpMethod::Post,
        SemanticHttpMethod::Put => HttpMethod::Put,
        SemanticHttpMethod::Patch => HttpMethod::Patch,
        SemanticHttpMethod::Delete => HttpMethod::Delete,
        SemanticHttpMethod::Head => HttpMethod::Head,
        SemanticHttpMethod::Options => HttpMethod::Options,
        SemanticHttpMethod::Trace => HttpMethod::Trace,
    }
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
) -> Result<(), LowerError> {
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
            }
            .into());
        }
    }

    for name in declared {
        if !placeholders.contains(name) {
            return Err(ValidationError::UnusedPathParameter {
                path: path.to_owned(),
                name: name.to_owned(),
            }
            .into());
        }
    }

    Ok(())
}

pub(in crate::parse) fn path_parameter_names(path: &str) -> Result<BTreeSet<String>, LowerError> {
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

fn parse_path_segments(path: &str) -> Result<Vec<PathSegment>, LowerError> {
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
    helpers::inferred_operation_id(method.operation_prefix(), path)
}
