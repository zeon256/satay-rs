use std::collections::BTreeSet;

use oas3::spec::SecurityScheme as OasSecurityScheme;

use super::super::reference::resolve_security_scheme;
use super::super::resolve::ResolvedDocument;
use super::schema::SchemaLowerer;
use crate::error::ValidationError;
use crate::ident::{
    field_ident, function_ident, response_range_variant_ident, response_variant_ident, type_ident,
    unique_ident,
};
use crate::model::{
    ApiKeyLocation, ApiKeySecurityScheme, Operation as SatayOperation, Parameter,
    ParameterLocation, RequestBody, ResponseCase, ResponseStatus, is_array_type,
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
    schemas: &mut SchemaLowerer<'_, '_>,
) -> Result<Vec<SatayOperation>, ValidationError> {
    document
        .operations
        .iter()
        .map(|operation| parse_operation(operation, registry, schemas))
        .collect()
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
