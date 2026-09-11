use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;

use oas3::{
    Map as OasMap,
    spec::{
        ObjectOrReference, ObjectSchema as OasObjectSchema, Operation as OasOperation,
        Parameter as OasParameter, ParameterIn as OasParameterIn, RequestBody as OasRequestBody,
        Response as OasResponse, Schema as OasSchema, SchemaType as OasSchemaType,
    },
};

use super::super::helpers::{json_media_type, optional_description};
use super::super::reference::schema_type_and_nullable;
use super::super::resolve::ResolvedDocument;
use super::super::satay::{SatayOperationOptions, SatayOutputOptions, operation_options};
use super::constraint::parse_validation;
use super::schema::{
    inline_union_null_branch, reject_any_of_sibling_keywords, reject_plain_one_of_sibling_keywords,
    schema_uses_all_of, schema_uses_any_of, validate_value_schema,
};
use super::{
    ValidatedOperation, ValidatedParameter, ValidatedRequestBody, ValidatedResponse,
    ValidatedResponseProjection, ValidatedType, ValidatedTypeKind,
};
use crate::ident::variant_ident;
use crate::model::{
    Enum, EnumFallback, HttpMethod, IntegerType, ParameterDefault, ParameterLocation, PathSegment,
    ResponseStatus, TypeRef, Validation,
};
use crate::{error::ValidationError, model::FloatLimit};

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
        let path_item = document.resolve_path_item(path_item, &format!("path item `{path}`"))?;

        let mut present = 0usize;
        let mut retained: Vec<(HttpMethod, &OasOperation, String, SatayOperationOptions)> = vec![];

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
            let Some(operation) = operation else {
                continue;
            };
            present += 1;
            let operation_id = operation
                .operation_id
                .clone()
                .unwrap_or_else(|| inferred_operation_id(method, path));
            let context = format!("operation `{operation_id}`");
            let options = operation_options(operation, &context)?.unwrap_or_default();
            if options.skip {
                continue;
            }
            retained.push((method, operation, operation_id, options));
        }

        // Path has operations and every one is skipped: do not validate the
        // path-level parameters and produce no operations for this path.
        if present >= 1 && retained.is_empty() {
            continue;
        }

        let path_parameters = validate_parameter_list(
            document,
            &path_item.parameters,
            &format!("path item `{path}` parameters"),
        )?;

        for (method, operation, operation_id, options) in retained {
            operations.push(validate_operation(
                document,
                method,
                path,
                &path_parameters,
                operation,
                operation_id,
                options,
            )?);
        }
    }

    Ok(operations)
}

fn validate_operation(
    document: &ResolvedDocument<'_>,
    method: HttpMethod,
    path: &str,
    path_parameters: &[ValidatedParameter],
    operation: &OasOperation,
    operation_id: String,
    options: SatayOperationOptions,
) -> Result<ValidatedOperation, ValidationError> {
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
        options.output.as_ref(),
    )?;

    if options.output.is_some() && responses.iter().all(|response| response.body.is_none()) {
        return Err(ValidationError::SatayOutputRequiresResponseBody {
            operation_id: operation_id.clone(),
        });
    }

    Ok(ValidatedOperation {
        operation_id,
        tags: operation.tags.clone(),
        description: optional_description(&operation.description),
        method,
        path: path.to_owned(),
        path_segments: parse_path_segments(path)?,
        parameters,
        request_body,
        responses,
    })
}

/// Reads whether the operation-level `x-satay` extension skips this operation.
pub(super) fn operation_satay_skip(
    operation: &OasOperation,
    operation_id: &str,
) -> Result<bool, ValidationError> {
    let context = format!("operation `{operation_id}`");
    let options = operation_options(operation, &context)?.unwrap_or_default();
    Ok(options.skip)
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
    let parameter = document.resolve(parameter, context)?;
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

    let declared_schema =
        parameter
            .schema
            .as_ref()
            .ok_or_else(|| ValidationError::MissingParameterSchema {
                context: context.to_owned(),
                wire_name: wire_name.clone(),
            })?;

    let required = match location {
        ParameterLocation::Path => {
            if parameter.required != Some(true) {
                return Err(ValidationError::PathParameterNotRequired { wire_name });
            }
            true
        }
        ParameterLocation::Query | ParameterLocation::Header => parameter.required.unwrap_or(false),
    };

    let schema_context = format!("parameter `{wire_name}`");
    let (schema, nullable_schema_peeled) =
        peel_nullable_parameter_schema(declared_schema, required, location, &schema_context)?;

    if schema_uses_all_of(document, schema)? {
        return Err(ValidationError::UnsupportedComposition {
            context: schema_context.clone(),
            keyword: "allOf",
        });
    }

    let mut ty = validate_value_schema(document, schema, &schema_context)?;
    let default_allows_null = ty.is_nullable() || nullable_schema_peeled;

    if ty.is_nullable() {
        if !required
            && matches!(
                location,
                ParameterLocation::Query | ParameterLocation::Header
            )
        {
            // Absent and null mean the same thing on the wire for optional
            // query/header parameters: fold nullability into optionality;
            // `required: false` already renders `Option<T>`.
            ty.nullable = false;
        } else {
            return Err(ValidationError::NullableParameterUnsupported {
                wire_name: wire_name.clone(),
            });
        }
    }

    validate_parameter_encoding(document, schema, &ty, location, &wire_name, &schema_context)?;
    let default = if required {
        None
    } else {
        validate_parameter_default(
            document,
            declared_schema,
            schema,
            &ty,
            default_allows_null,
            &wire_name,
            &schema_context,
        )?
    };

    Ok(ValidatedParameter {
        location,
        wire_name: parameter.name.clone(),
        description: optional_description(&parameter.description),
        ty,
        required,
        default,
    })
}
fn validate_parameter_encoding(
    document: &ResolvedDocument<'_>,
    schema: &OasSchema,
    ty: &ValidatedType,
    location: ParameterLocation,
    wire_name: &str,
    context: &str,
) -> Result<(), ValidationError> {
    if ty.contains_inline_struct() {
        return Err(ValidationError::UnsupportedComposition {
            context: context.to_owned(),
            keyword: "allOf",
        });
    }
    if ty.contains_any_of() || schema_uses_any_of(document, schema)? {
        return Err(ValidationError::AnyOfParameterUnsupported {
            wire_name: wire_name.to_owned(),
        });
    }
    if ty.contains_map_or_json_value() {
        return Err(ValidationError::MapParameterUnsupported {
            wire_name: wire_name.to_owned(),
        });
    }
    if location == ParameterLocation::Path && ty.is_array() {
        return Err(ValidationError::ArrayPathParameterUnsupported {
            wire_name: wire_name.to_owned(),
        });
    }
    if location == ParameterLocation::Header && ty.is_array() {
        return Err(ValidationError::ArrayHeaderParameterUnsupported {
            wire_name: wire_name.to_owned(),
        });
    }
    Ok(())
}

/// Returns the non-null branch of an optional query/header parameter shaped
/// `anyOf`/`oneOf`: `[T, {type: "null"}]` (FastAPI `Optional[T]`), so its
/// nullability folds into optionality. Any other shape is returned unchanged.
fn peel_nullable_parameter_schema<'a>(
    schema: &'a OasSchema,
    required: bool,
    location: ParameterLocation,
    context: &str,
) -> Result<(&'a OasSchema, bool), ValidationError> {
    if required
        || !matches!(
            location,
            ParameterLocation::Query | ParameterLocation::Header
        )
    {
        return Ok((schema, false));
    }
    if schema.reference().is_some() {
        return Ok((schema, false));
    }
    let Some(object) = schema.as_object() else {
        return Ok((schema, false));
    };

    let (branches, is_any_of) = match (object.any_of.as_slice(), object.one_of.as_slice()) {
        (branches @ [_, _], []) => (branches, true),
        ([], branches @ [_, _]) => (branches, false),
        _ => return Ok((schema, false)),
    };

    let is_null_branch =
        |branch: &OasSchema| branch.as_object().is_some_and(inline_union_null_branch);

    let non_null_branch = match (is_null_branch(&branches[0]), is_null_branch(&branches[1])) {
        (false, true) => &branches[0],
        (true, false) => &branches[1],
        _ => return Ok((schema, false)),
    };

    if is_any_of {
        reject_any_of_sibling_keywords(object, context)?;
    } else {
        reject_plain_one_of_sibling_keywords(object, context)?;
    }

    Ok((non_null_branch, true))
}
fn validate_parameter_default(
    document: &ResolvedDocument<'_>,
    declared_schema: &OasSchema,
    effective_schema: &OasSchema,
    ty: &ValidatedType,
    allows_null: bool,
    wire_name: &str,
    context: &str,
) -> Result<Option<ParameterDefault>, ValidationError> {
    let resolved_schema = document.resolve_schema(effective_schema, context)?;
    let value = declared_schema
        .as_object()
        .and_then(|schema| schema.default.as_ref())
        .or_else(|| {
            effective_schema
                .as_object()
                .and_then(|schema| schema.default.as_ref())
        })
        .or_else(|| {
            resolved_schema
                .as_object()
                .and_then(|schema| schema.default.as_ref())
        });
    let Some(value) = value else {
        return Ok(None);
    };

    if value.is_null() {
        return if allows_null {
            Ok(None)
        } else {
            Err(invalid_parameter_default(
                wire_name,
                value,
                "null is not valid for this parameter",
            ))
        };
    }

    let resolved_ty;
    let ty = if matches!(ty.kind, ValidatedTypeKind::Named(_)) {
        resolved_ty = validate_value_schema(document, resolved_schema, context)?;
        &resolved_ty
    } else {
        ty
    };

    let default = parse_parameter_default(value, ty, wire_name)?;
    let enum_validation = match &ty.kind {
        ValidatedTypeKind::Enum(_) => resolved_schema
            .as_object()
            .map(|schema| parse_validation(schema, &TypeRef::String, context))
            .transpose()?
            .flatten(),
        _ => None,
    };
    let validation = ty.validation.as_ref().or(enum_validation.as_ref());

    validate_parameter_default_constraints(&default, validation)
        .map_err(|reason| invalid_parameter_default(wire_name, value, reason))?;
    Ok(Some(default))
}
fn parse_parameter_default(
    value: &JsonValue,
    ty: &ValidatedType,
    wire_name: &str,
) -> Result<ParameterDefault, ValidationError> {
    match &ty.kind {
        ValidatedTypeKind::String => Ok(ParameterDefault::String(
            value
                .as_str()
                .ok_or_else(|| {
                    invalid_parameter_default(wire_name, value, "expected a JSON string")
                })?
                .to_owned(),
        )),
        ValidatedTypeKind::Integer(integer_type) => {
            parse_integer_parameter_default(value, *integer_type, wire_name)
        }
        ValidatedTypeKind::F32 => parse_number_parameter_default(value, true, wire_name),
        ValidatedTypeKind::F64 => parse_number_parameter_default(value, false, wire_name),
        ValidatedTypeKind::Bool => Ok(ParameterDefault::Bool(value.as_bool().ok_or_else(
            || invalid_parameter_default(wire_name, value, "expected a JSON boolean"),
        )?)),
        ValidatedTypeKind::Enum(enum_) => parse_enum_parameter_default(value, enum_, wire_name),
        ValidatedTypeKind::ParsedString(_)
        | ValidatedTypeKind::Coordinates(_)
        | ValidatedTypeKind::ParsedInteger(_) => Err(invalid_parameter_default(
            wire_name,
            value,
            "defaults for x-satay parsed parameters are not supported",
        )),
        ValidatedTypeKind::Array(_) => Err(invalid_parameter_default(
            wire_name,
            value,
            "array parameter defaults are not supported",
        )),
        ValidatedTypeKind::Range(_) => Err(invalid_parameter_default(
            wire_name,
            value,
            "range parameter defaults are not supported",
        )),
        ValidatedTypeKind::Named(_)
        | ValidatedTypeKind::Map(_)
        | ValidatedTypeKind::JsonValue
        | ValidatedTypeKind::AnyOf(_)
        | ValidatedTypeKind::InlineStruct(_) => Err(invalid_parameter_default(
            wire_name,
            value,
            "default is not supported for this parameter type",
        )),
    }
}

fn parse_integer_parameter_default(
    value: &JsonValue,
    integer_type: IntegerType,
    wire_name: &str,
) -> Result<ParameterDefault, ValidationError> {
    let integer = parameter_default_integer(value)
        .ok_or_else(|| invalid_parameter_default(wire_name, value, "expected a JSON integer"))?;
    if integer < integer_type.min_value() || integer > integer_type.max_value() {
        return Err(invalid_parameter_default(
            wire_name,
            value,
            format!(
                "value is outside the generated integer range {}..={}",
                integer_type.min_value(),
                integer_type.max_value()
            ),
        ));
    }
    Ok(ParameterDefault::Integer(integer))
}

fn parse_number_parameter_default(
    value: &JsonValue,
    is_f32: bool,
    wire_name: &str,
) -> Result<ParameterDefault, ValidationError> {
    let number = value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| {
            invalid_parameter_default(wire_name, value, "expected a finite JSON number")
        })?;
    if is_f32 {
        let number = number as f32;
        if !number.is_finite() {
            return Err(invalid_parameter_default(
                wire_name,
                value,
                "value is outside the generated f32 range",
            ));
        }
        Ok(ParameterDefault::F32(number))
    } else {
        Ok(ParameterDefault::F64(number))
    }
}

fn parse_enum_parameter_default(
    value: &JsonValue,
    enum_: &Enum,
    wire_name: &str,
) -> Result<ParameterDefault, ValidationError> {
    let wire_value = value.as_str().ok_or_else(|| {
        invalid_parameter_default(wire_name, value, "expected a JSON string enum value")
    })?;
    let default_empty_variant = variant_ident("");
    match enum_
        .variants
        .iter()
        .find(|variant| variant.wire_name == wire_value)
    {
        Some(variant)
            if enum_.variants.len() > 1
                && variant.wire_name.is_empty()
                && variant.rust_name == default_empty_variant =>
        {
            Err(invalid_parameter_default(
                wire_name,
                value,
                "empty enum default cannot be represented by the generated parameter enum",
            ))
        }
        Some(variant) => Ok(ParameterDefault::EnumVariant {
            wire_value: wire_value.to_owned(),
            rust_name: variant.rust_name.clone(),
        }),
        None if enum_.fallback == EnumFallback::OtherString => {
            Ok(ParameterDefault::OpenEnum(wire_value.to_owned()))
        }
        None => Err(invalid_parameter_default(
            wire_name,
            value,
            "value is not a declared enum variant",
        )),
    }
}

fn parameter_default_integer(value: &JsonValue) -> Option<i128> {
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
}

fn validate_parameter_default_constraints(
    default: &ParameterDefault,
    validation: Option<&Validation>,
) -> Result<(), String> {
    let Some(validation) = validation else {
        return Ok(());
    };

    match (default, validation) {
        (
            ParameterDefault::String(value)
            | ParameterDefault::EnumVariant {
                wire_value: value, ..
            }
            | ParameterDefault::OpenEnum(value),
            Validation::String {
                min_length,
                max_length,
                pattern,
            },
        ) => {
            let length = u64::try_from(value.chars().count()).unwrap_or(u64::MAX);
            if min_length.is_some_and(|minimum| length < minimum) {
                return Err(format!("string length {length} is below minLength"));
            }
            if max_length.is_some_and(|maximum| length > maximum) {
                return Err(format!("string length {length} exceeds maxLength"));
            }
            if let Some(pattern) = pattern {
                let regex = Regex::new(pattern)
                    .map_err(|error| format!("schema pattern is invalid: {error}"))?;
                if !regex.is_match(value) {
                    return Err(format!("string does not match pattern `{pattern}`"));
                }
            }
        }
        (ParameterDefault::Integer(value), Validation::Integer { minimum, maximum }) => {
            if minimum.is_some_and(|limit| {
                if limit.exclusive {
                    *value <= limit.value
                } else {
                    *value < limit.value
                }
            }) {
                return Err("integer is below the schema minimum".to_owned());
            }
            if maximum.is_some_and(|limit| {
                if limit.exclusive {
                    *value >= limit.value
                } else {
                    *value > limit.value
                }
            }) {
                return Err("integer exceeds the schema maximum".to_owned());
            }
        }
        (ParameterDefault::F32(value), Validation::Number { minimum, maximum }) => {
            validate_number_default_constraints(f64::from(*value), *minimum, *maximum, true)?;
        }
        (ParameterDefault::F64(value), Validation::Number { minimum, maximum }) => {
            validate_number_default_constraints(*value, *minimum, *maximum, false)?;
        }
        (_, Validation::Array { .. }) => {
            unreachable!("array parameter defaults are rejected before constraint validation")
        }
        _ => unreachable!("validated parameter default constraints must match the parameter type"),
    }

    Ok(())
}

fn validate_number_default_constraints(
    value: f64,
    minimum: Option<FloatLimit>,
    maximum: Option<FloatLimit>,
    is_f32: bool,
) -> Result<(), String> {
    let generated_value = |value: f64| {
        if is_f32 {
            f64::from(value as f32)
        } else {
            value
        }
    };
    let value = generated_value(value);

    if minimum.is_some_and(|limit| {
        let minimum = generated_value(limit.value);
        if limit.exclusive {
            value <= minimum
        } else {
            value < minimum
        }
    }) {
        return Err("number is below the schema minimum".to_owned());
    }
    if maximum.is_some_and(|limit| {
        let maximum = generated_value(limit.value);
        if limit.exclusive {
            value >= maximum
        } else {
            value > maximum
        }
    }) {
        return Err("number exceeds the schema maximum".to_owned());
    }

    Ok(())
}

fn invalid_parameter_default(
    wire_name: &str,
    value: &JsonValue,
    reason: impl Into<String>,
) -> ValidationError {
    ValidationError::InvalidParameterDefault {
        wire_name: wire_name.to_owned(),
        value: value.to_string(),
        reason: reason.into(),
    }
}

fn validate_request_body(
    document: &ResolvedDocument<'_>,
    request_body: Option<&ObjectOrReference<OasRequestBody>>,
    context: &str,
) -> Result<Option<ValidatedRequestBody>, ValidationError> {
    let Some(request_body) = request_body else {
        return Ok(None);
    };

    let request_body = document.resolve(request_body, context)?;

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
        ty: validate_value_schema(document, schema, context)?,
        required: request_body.required.unwrap_or(false),
    }))
}

fn validate_responses(
    document: &ResolvedDocument<'_>,
    responses: &OasMap<String, ObjectOrReference<OasResponse>>,
    context: &str,
    output: Option<&SatayOutputOptions>,
) -> Result<Vec<ValidatedResponse>, ValidationError> {
    let mut parsed = vec![];

    for (status, response) in responses {
        if status == "default" {
            let response = document.resolve(response, &format!("{context} default"))?;
            if !response.content.is_empty() {
                return Err(ValidationError::DefaultResponseBodyUnsupported {
                    context: context.to_owned(),
                });
            }
            continue;
        }

        let parsed_status = if let Some(class) = wildcard_status_class(status) {
            ResponseStatus::Range(class)
        } else {
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
            ResponseStatus::Exact(status_code)
        };

        let response = document.resolve(response, &format!("{context} {status}"))?;

        let (body, projection) = if response.content.is_empty() {
            (None, None)
        } else {
            let (_, media_type) = json_media_type(&response.content).ok_or_else(|| {
                ValidationError::MissingResponseJsonContent {
                    context: context.to_owned(),
                    status: status.to_owned(),
                }
            })?;
            match media_type.schema.as_ref() {
                Some(schema) => {
                    let context = format!("{context} {status} schema");
                    let body = match output {
                        Some(output) => {
                            validate_projected_response_type(document, schema, output, &context)?
                        }
                        None => validate_value_schema(document, schema, &context)?,
                    };
                    let projection = output.map(|output| ValidatedResponseProjection {
                        unwrap_field: output.unwrap_field.as_str().to_owned(),
                        map_field: output
                            .map_field
                            .as_ref()
                            .map(|field| field.as_str().to_owned()),
                    });
                    (Some(body), projection)
                }
                None => (None, None),
            }
        };

        parsed.push(ValidatedResponse {
            status: parsed_status,
            description: optional_description(&response.description),
            body,
            projection,
        });
    }

    parsed.sort_by_key(|response| response.status);
    Ok(parsed)
}

fn validate_projected_response_type(
    document: &ResolvedDocument<'_>,
    response_schema: &OasSchema,
    output: &SatayOutputOptions,
    context: &str,
) -> Result<ValidatedType, ValidationError> {
    let wrapper = projected_object_schema(document, response_schema, context, "unwrap-field")?;
    let unwrap_field = output.unwrap_field.as_str();
    let unwrapped = wrapper.properties.get(unwrap_field).ok_or_else(|| {
        ValidationError::UnknownSatayOutputField {
            context: context.to_owned(),
            selector: "unwrap-field",
            field: unwrap_field.to_owned(),
        }
    })?;
    let unwrap_required = wrapper.required.iter().any(|field| field == unwrap_field);

    let Some(map_field) = output.map_field.as_ref() else {
        let mut ty = validate_value_schema(document, unwrapped, context)?;
        if !unwrap_required {
            ty.nullable = true;
        }
        return Ok(ty);
    };

    let array_schema = document.resolve_schema(unwrapped, context)?;
    let array =
        array_schema
            .as_object()
            .ok_or_else(|| ValidationError::SatayOutputMapRequiresArray {
                context: context.to_owned(),
                field: unwrap_field.to_owned(),
            })?;
    let (schema_type, _) = schema_type_and_nullable(array, context)?;
    if schema_type != Some(OasSchemaType::Array) {
        return Err(ValidationError::SatayOutputMapRequiresArray {
            context: context.to_owned(),
            field: unwrap_field.to_owned(),
        });
    }
    let items = array
        .items
        .as_deref()
        .ok_or_else(|| ValidationError::MissingArrayItems {
            context: context.to_owned(),
        })?;
    let item = projected_object_schema(document, items, context, "map-field")?;
    let map_field = map_field.as_str();
    let mapped =
        item.properties
            .get(map_field)
            .ok_or_else(|| ValidationError::UnknownSatayOutputField {
                context: context.to_owned(),
                selector: "map-field",
                field: map_field.to_owned(),
            })?;
    let map_required = item.required.iter().any(|field| field == map_field);

    let mut projected_array = array.clone();
    projected_array.items = Some(Box::new(mapped.clone()));
    let projected_array = OasSchema::Object(Box::new(projected_array));
    let mut ty = validate_value_schema(document, &projected_array, context)?;
    if !unwrap_required {
        ty.nullable = true;
    }
    let ValidatedTypeKind::Array(item) = &mut ty.kind else {
        unreachable!("projected array schema validates as an array")
    };
    if !map_required {
        item.nullable = true;
    }
    Ok(ty)
}

fn projected_object_schema<'a>(
    document: &'a ResolvedDocument<'_>,
    schema: &'a OasSchema,
    context: &str,
    selector: &'static str,
) -> Result<&'a OasObjectSchema, ValidationError> {
    let schema = document.resolve_schema(schema, context)?;
    let object = schema
        .as_object()
        .ok_or_else(|| ValidationError::SatayOutputExpectedObject {
            context: context.to_owned(),
            selector,
        })?;
    let (schema_type, _) = schema_type_and_nullable(object, context)?;
    if !matches!(schema_type, Some(OasSchemaType::Object) | None) || object.properties.is_empty() {
        return Err(ValidationError::SatayOutputExpectedObject {
            context: context.to_owned(),
            selector,
        });
    }
    Ok(object)
}

/// Matches OpenAPI wildcard response keys `1XX`..`5XX` (uppercase only).
fn wildcard_status_class(status: &str) -> Option<u8> {
    match status.as_bytes() {
        [class @ b'1'..=b'5', b'X', b'X'] => Some(class - b'0'),
        _ => None,
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

pub(super) fn inferred_operation_id(method: HttpMethod, path: &str) -> String {
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
