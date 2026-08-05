use std::collections::BTreeSet;

use oas3::spec::{SecurityScheme as OasSecurityScheme, Spec as OasSpec};

use super::super::resolve::ResolvedDocument;
use super::schema::SchemaLowerer;
use crate::error::ValidationError;
use crate::ident::{
    field_ident, function_ident, group_ident, response_range_variant_ident, response_variant_ident,
    type_ident, unique_ident,
};
use crate::model::{
    ApiGroup, ApiKeyLocation, ApiKeySecurityScheme, GroupOperation, Operation as SatayOperation,
    Parameter, ParameterLocation, RequestBody, ResponseCase, ResponseStatus, is_array_type,
};
use crate::parse::registry::TypeRegistry;
use crate::parse::validate::{
    ValidatedDocument, ValidatedOperation, ValidatedParameter, ValidatedRequestBody,
    ValidatedResponse,
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
        let scheme = document.resolve(scheme, &context)?;
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
    schemas: &mut SchemaLowerer<'_, '_>,
) -> Result<Vec<SatayOperation>, ValidationError> {
    document
        .operations
        .iter()
        .map(|operation| parse_operation(operation, registry, schemas))
        .collect()
}

pub(super) fn parse_api_groups(
    spec: &OasSpec,
    api_key_security_schemes: &[ApiKeySecurityScheme],
    operations: &[SatayOperation],
) -> Vec<ApiGroup> {
    let used_tags = operations
        .iter()
        .flat_map(|operation| operation.tags.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mut ordered_tags = vec![];
    let mut seen_tags = BTreeSet::new();

    // Root tag order is meaningful in OpenAPI. Undeclared tags follow in
    // first-operation order so generation stays deterministic.
    for tag in &spec.tags {
        if used_tags.contains(&tag.name) && seen_tags.insert(tag.name.clone()) {
            ordered_tags.push(tag.name.clone());
        }
    }
    for operation in operations {
        for tag in &operation.tags {
            if seen_tags.insert(tag.clone()) {
                ordered_tags.push(tag.clone());
            }
        }
    }

    let mut used_modules = BTreeSet::from(["api".to_owned(), "types".to_owned()]);
    used_modules.extend(operations.iter().map(|operation| operation.fn_name.clone()));

    let mut used_accessors =
        BTreeSet::from(["apply".to_owned(), "base_url".to_owned(), "new".to_owned()]);
    used_accessors.extend(
        api_key_security_schemes
            .iter()
            .map(|scheme| scheme.rust_name.clone()),
    );

    let mut groups = ordered_tags
        .into_iter()
        .map(|tag_name| {
            let base_name = group_ident(&tag_name);
            let rust_name = unique_group_ident(&base_name, &mut used_modules, &mut used_accessors);
            let description = spec
                .tags
                .iter()
                .find(|tag| tag.name == tag_name)
                .and_then(|tag| tag.description.clone());
            build_group(
                Some(tag_name),
                rust_name,
                description,
                Some(&base_name),
                operations,
            )
        })
        .collect::<Vec<_>>();

    if operations.iter().any(|operation| operation.tags.is_empty()) {
        let rust_name = unique_group_ident("untagged", &mut used_modules, &mut used_accessors);
        groups.push(build_group(
            None,
            rust_name,
            Some("Operations without an OpenAPI tag.".to_owned()),
            None,
            operations,
        ));
    }

    groups
}

fn build_group(
    wire_name: Option<String>,
    rust_name: String,
    description: Option<String>,
    group_base_name: Option<&str>,
    operations: &[SatayOperation],
) -> ApiGroup {
    let mut used_methods = BTreeSet::new();
    let operations = operations
        .iter()
        .enumerate()
        .filter(|(_, operation)| match &wire_name {
            Some(tag) => operation.tags.contains(tag),
            None => operation.tags.is_empty(),
        })
        .map(|(operation_index, operation)| {
            let candidate = group_base_name
                .and_then(|group| strip_group_from_operation(&operation.fn_name, group))
                .unwrap_or_else(|| operation.fn_name.clone());
            GroupOperation {
                operation_index,
                method_name: unique_ident(candidate, &mut used_methods),
            }
        })
        .collect();

    ApiGroup {
        wire_name,
        rust_name,
        description,
        operations,
    }
}

fn strip_group_from_operation(operation: &str, group: &str) -> Option<String> {
    let operation_parts = operation
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let group_parts = group
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if group_parts.is_empty() || group_parts.len() >= operation_parts.len() {
        return None;
    }

    let start = operation_parts
        .windows(group_parts.len())
        .position(|window| window == group_parts)?;
    let shortened = operation_parts[..start]
        .iter()
        .chain(&operation_parts[start + group_parts.len()..])
        .copied()
        .collect::<Vec<_>>()
        .join("_");
    (!shortened.is_empty()).then(|| function_ident(&shortened))
}

fn unique_group_ident(
    candidate: &str,
    used_modules: &mut BTreeSet<String>,
    used_accessors: &mut BTreeSet<String>,
) -> String {
    for suffix in 1.. {
        let next = if suffix == 1 {
            candidate.to_owned()
        } else {
            format!("{candidate}_{suffix}")
        };
        if !used_modules.contains(&next) && !used_accessors.contains(&next) {
            used_modules.insert(next.clone());
            used_accessors.insert(next.clone());
            return next;
        }
    }
    unreachable!()
}

fn parse_operation(
    operation: &ValidatedOperation,
    registry: &mut TypeRegistry,
    schemas: &mut SchemaLowerer<'_, '_>,
) -> Result<SatayOperation, ValidationError> {
    let fn_name = function_ident(&operation.operation_id);
    let type_prefix = type_ident(&operation.operation_id);
    let input_name = registry.reserve_preferred_type_name([format!("{type_prefix}Input")]);
    let response_name = registry.reserve_preferred_type_name([
        format!("{type_prefix}Response"),
        format!("{type_prefix}OperationResponse"),
    ]);

    let mut parameters = operation
        .parameters
        .iter()
        .map(|parameter| parse_parameter(parameter, registry, schemas, &type_prefix))
        .collect::<Result<Vec<_>, _>>()?;
    deduplicate_parameter_fields(&mut parameters);

    let request_body = parse_request_body(
        operation.request_body.as_ref(),
        &parameters,
        registry,
        schemas,
        &type_prefix,
    );

    let responses = operation
        .responses
        .iter()
        .map(|response| parse_response(response, registry, schemas, &type_prefix))
        .collect();

    Ok(SatayOperation {
        fn_name,
        tags: operation.tags.clone(),
        description: operation.description.clone(),
        input_name,
        response_name,
        method: operation.method,
        path: operation.path.clone(),
        path_segments: operation.path_segments.clone(),
        parameters,
        request_body,
        responses,
    })
}

fn parse_parameter(
    parameter: &ValidatedParameter,
    registry: &mut TypeRegistry,
    schemas: &mut SchemaLowerer<'_, '_>,
    type_prefix: &str,
) -> Result<Parameter, ValidationError> {
    let ty = schemas.parse_type_ref_with_hint(
        &parameter.ty,
        &format!("{type_prefix} {} parameter", parameter.wire_name),
        registry,
    );

    if ty.is_option() {
        return Err(ValidationError::NullableParameterUnsupported {
            wire_name: parameter.wire_name.clone(),
        });
    }

    if parameter.location == ParameterLocation::Path && is_array_type(&ty) {
        return Err(ValidationError::ArrayPathParameterUnsupported {
            wire_name: parameter.wire_name.clone(),
        });
    }

    if parameter.location == ParameterLocation::Header && is_array_type(&ty) {
        return Err(ValidationError::ArrayHeaderParameterUnsupported {
            wire_name: parameter.wire_name.clone(),
        });
    }

    Ok(Parameter {
        location: parameter.location,
        wire_name: parameter.wire_name.clone(),
        rust_name: field_ident(&parameter.wire_name),
        description: parameter.description.clone(),
        ty,
        required: parameter.required,
    })
}

fn deduplicate_parameter_fields(parameters: &mut [Parameter]) {
    let mut used = BTreeSet::new();
    for parameter in parameters {
        parameter.rust_name = unique_ident(parameter.rust_name.clone(), &mut used);
    }
}

fn parse_request_body(
    request_body: Option<&ValidatedRequestBody>,
    parameters: &[Parameter],
    registry: &mut TypeRegistry,
    schemas: &mut SchemaLowerer<'_, '_>,
    type_prefix: &str,
) -> Option<RequestBody> {
    let request_body = request_body?;

    let mut used = parameters
        .iter()
        .map(|parameter| parameter.rust_name.clone())
        .collect::<BTreeSet<_>>();
    let field_name = unique_ident("body".to_owned(), &mut used);

    Some(RequestBody {
        field_name,
        description: request_body.description.clone(),
        content_type: request_body.content_type.clone(),
        ty: schemas.parse_type_ref_with_hint(
            &request_body.ty,
            &format!("{type_prefix} request body"),
            registry,
        ),
        required: request_body.required,
    })
}

fn parse_response(
    response: &ValidatedResponse,
    registry: &mut TypeRegistry,
    schemas: &mut SchemaLowerer<'_, '_>,
    type_prefix: &str,
) -> ResponseCase {
    ResponseCase {
        status: response.status,
        variant_name: match response.status {
            ResponseStatus::Exact(code) => response_variant_ident(code),
            ResponseStatus::Range(class) => response_range_variant_ident(class),
        },
        description: response.description.clone(),
        body: response.body.as_ref().map(|body| {
            schemas.parse_type_ref_with_hint(
                body,
                &format!("{type_prefix} response {}", response.status),
                registry,
            )
        }),
    }
}
