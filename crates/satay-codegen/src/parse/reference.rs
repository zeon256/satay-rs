use oas3::spec::{
    ObjectOrReference, ObjectSchema as OasObjectSchema, Parameter as OasParameter,
    PathItem as OasPathItem, RequestBody as OasRequestBody, Response as OasResponse,
    Schema as OasSchema, SchemaType as OasSchemaType, SchemaTypeSet as OasSchemaTypeSet,
    SecurityScheme as OasSecurityScheme,
};
use serde_json::Value;

use super::Document;
use crate::error::ValidationError;
use crate::ident::type_ident;

pub(super) fn schema_ref<'a>(
    schema: &'a OasSchema,
    raw_schema: Option<&Value>,
    context: &str,
) -> Result<Option<&'a str>, ValidationError> {
    reject_legacy_nullable(raw_schema, context)?;
    reject_raw_ref_siblings(raw_schema, context, true)?;
    match schema {
        OasSchema::Boolean(_) => Ok(None),
        OasSchema::Object(object) => match object.as_ref() {
            ObjectOrReference::Ref { ref_path, .. } => Ok(Some(ref_path.as_str())),
            ObjectOrReference::Object(_) => Ok(None),
        },
    }
}

pub(super) fn schema_ref_type_name(reference: &str) -> Result<String, ValidationError> {
    let name = local_ref_name(reference, "schemas")?;
    Ok(type_ident(&name))
}

pub(super) fn object_schema<'a>(
    schema: &'a OasSchema,
    context: &str,
) -> Result<&'a OasObjectSchema, ValidationError> {
    match schema {
        OasSchema::Boolean(_) => Err(ValidationError::UnsupportedBooleanSchema {
            context: context.to_owned(),
        }),
        OasSchema::Object(object) => match object.as_ref() {
            ObjectOrReference::Object(schema) => Ok(schema),
            ObjectOrReference::Ref { .. } => Err(ValidationError::ExpectedObject {
                context: context.to_owned(),
            }),
        },
    }
}

pub(super) fn schema_type_and_nullable(
    schema: &OasObjectSchema,
    raw_schema: Option<&Value>,
    context: &str,
) -> Result<(Option<OasSchemaType>, bool), ValidationError> {
    reject_legacy_nullable(raw_schema, context)?;
    let Some(schema_type) = schema.schema_type.as_ref() else {
        return Ok((None, false));
    };

    match schema_type {
        OasSchemaTypeSet::Single(OasSchemaType::Null) => {
            Err(ValidationError::UnsupportedSchemaType {
                context: context.to_owned(),
                kind: "null".to_owned(),
            })
        }
        OasSchemaTypeSet::Single(schema_type) => Ok((Some(*schema_type), false)),
        OasSchemaTypeSet::Multiple(types) => {
            let mut nullable = false;
            let mut non_null = None;
            let mut non_null_count = 0usize;
            for schema_type in types {
                if *schema_type == OasSchemaType::Null {
                    nullable = true;
                } else {
                    non_null_count += 1;
                    if non_null.is_none() {
                        non_null = Some(*schema_type);
                    }
                }
            }

            match non_null_count {
                0 => Err(ValidationError::UnsupportedSchemaType {
                    context: context.to_owned(),
                    kind: "null".to_owned(),
                }),
                1 => Ok((non_null, nullable)),
                _ => Err(ValidationError::MultipleNonNullSchemaTypesUnsupported {
                    context: context.to_owned(),
                }),
            }
        }
    }
}

pub(super) fn schema_type_wire(schema_type: OasSchemaType) -> &'static str {
    match schema_type {
        OasSchemaType::Boolean => "boolean",
        OasSchemaType::Integer => "integer",
        OasSchemaType::Number => "number",
        OasSchemaType::String => "string",
        OasSchemaType::Array => "array",
        OasSchemaType::Object => "object",
        OasSchemaType::Null => "null",
    }
}

pub(super) fn reject_legacy_nullable(
    raw_schema: Option<&Value>,
    context: &str,
) -> Result<(), ValidationError> {
    if raw_schema
        .and_then(Value::as_object)
        .is_some_and(|schema| schema.contains_key("nullable"))
    {
        return Err(ValidationError::UnsupportedNullableKeyword {
            context: context.to_owned(),
        });
    }
    Ok(())
}

fn reject_raw_ref_siblings(
    raw_value: Option<&Value>,
    context: &str,
    allow_satay: bool,
) -> Result<(), ValidationError> {
    let Some(object) = raw_value.and_then(Value::as_object) else {
        return Ok(());
    };

    let Some(reference) = object.get("$ref").and_then(Value::as_str) else {
        return Ok(());
    };

    for key in object.keys() {
        if key == "$ref" || (allow_satay && key == "x-satay") {
            continue;
        }
        return Err(ValidationError::RefSiblingUnsupported {
            context: context.to_owned(),
            reference: reference.to_owned(),
            sibling: key.clone(),
        });
    }

    Ok(())
}

pub(super) fn reject_composition(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    for (keyword, present) in [
        ("oneOf", !schema.one_of.is_empty()),
        ("anyOf", !schema.any_of.is_empty()),
        ("allOf", !schema.all_of.is_empty()),
    ] {
        if present {
            return Err(ValidationError::UnsupportedComposition {
                context: context.to_owned(),
                keyword,
            });
        }
    }
    Ok(())
}

pub(super) fn resolve_path_item<'a>(
    document: &'a Document,
    path_item: &'a OasPathItem,
    raw_path_item: Option<&'a Value>,
    context: &str,
) -> Result<(&'a OasPathItem, Option<&'a Value>), ValidationError> {
    let Some(reference) = path_item.reference.as_deref() else {
        return Ok((path_item, raw_path_item));
    };

    reject_raw_ref_siblings(raw_path_item, context, false)?;

    let name = local_ref_name(reference, "pathItems").map_err(|source| {
        ValidationError::ResolveReference {
            reference: reference.to_owned(),
            context: context.to_owned(),
            source: Box::new(source),
        }
    })?;

    let target = document
        .spec
        .as_ref()
        .and_then(|spec| spec.components.as_ref())
        .and_then(|components| components.path_items.get(&name))
        .ok_or_else(|| ValidationError::ResolveReference {
            reference: reference.to_owned(),
            context: context.to_owned(),
            source: Box::new(ValidationError::MissingJsonPointerToken {
                token: name.clone(),
            }),
        })?;
    let raw_target = resolve_json_pointer(&document.raw, reference).ok();

    resolve_path_item_reference(document, target, raw_target, context, reference)
}

fn resolve_path_item_reference<'a>(
    document: &'a Document,
    path_item: &'a ObjectOrReference<OasPathItem>,
    raw_path_item: Option<&'a Value>,
    context: &str,
    reference: &str,
) -> Result<(&'a OasPathItem, Option<&'a Value>), ValidationError> {
    match path_item {
        ObjectOrReference::Object(path_item) => {
            resolve_path_item(document, path_item, raw_path_item, context)
        }
        ObjectOrReference::Ref { ref_path, .. } => {
            reject_raw_ref_siblings(raw_path_item, context, false)?;
            let name = local_ref_name(ref_path, "pathItems").map_err(|source| {
                ValidationError::ResolveReference {
                    reference: reference.to_owned(),
                    context: context.to_owned(),
                    source: Box::new(source),
                }
            })?;
            let target = document
                .spec
                .as_ref()
                .and_then(|spec| spec.components.as_ref())
                .and_then(|components| components.path_items.get(&name))
                .ok_or_else(|| ValidationError::ResolveReference {
                    reference: reference.to_owned(),
                    context: context.to_owned(),
                    source: Box::new(ValidationError::MissingJsonPointerToken { token: name }),
                })?;
            let raw_target = resolve_json_pointer(&document.raw, ref_path).ok();
            resolve_path_item_reference(document, target, raw_target, context, reference)
        }
    }
}

pub(super) fn resolve_security_scheme<'a>(
    document: &'a Document,
    scheme: &'a ObjectOrReference<OasSecurityScheme>,
    raw_scheme: Option<&'a Value>,
    context: &str,
) -> Result<&'a OasSecurityScheme, ValidationError> {
    match scheme {
        ObjectOrReference::Object(scheme) => Ok(scheme),
        ObjectOrReference::Ref { ref_path, .. } => {
            reject_raw_ref_siblings(raw_scheme, context, false)?;
            let reference = ref_path.clone();
            resolve_security_scheme_ref(document, ref_path, context).map_err(|source| {
                ValidationError::ResolveReference {
                    reference,
                    context: context.to_owned(),
                    source: Box::new(source),
                }
            })
        }
    }
}

fn resolve_security_scheme_ref<'a>(
    document: &'a Document,
    reference: &str,
    context: &str,
) -> Result<&'a OasSecurityScheme, ValidationError> {
    let name = local_ref_name(reference, "securitySchemes")?;

    let target = document
        .spec
        .as_ref()
        .and_then(|spec| spec.components.as_ref())
        .and_then(|components| components.security_schemes.get(&name))
        .ok_or_else(|| ValidationError::MissingJsonPointerToken {
            token: name.clone(),
        })?;

    let raw_target = resolve_json_pointer(&document.raw, reference).ok();

    resolve_security_scheme(document, target, raw_target, context)
}

pub(super) fn resolve_parameter<'a>(
    document: &'a Document,
    parameter: &'a ObjectOrReference<OasParameter>,
    raw_parameter: Option<&'a Value>,
    context: &str,
) -> Result<(&'a OasParameter, Option<&'a Value>), ValidationError> {
    match parameter {
        ObjectOrReference::Object(parameter) => Ok((parameter, raw_parameter)),
        ObjectOrReference::Ref { ref_path, .. } => {
            reject_raw_ref_siblings(raw_parameter, context, false)?;
            let reference = ref_path.clone();
            resolve_parameter_ref(document, ref_path, context).map_err(|source| {
                ValidationError::ResolveReference {
                    reference,
                    context: context.to_owned(),
                    source: Box::new(source),
                }
            })
        }
    }
}

fn resolve_parameter_ref<'a>(
    document: &'a Document,
    reference: &str,
    context: &str,
) -> Result<(&'a OasParameter, Option<&'a Value>), ValidationError> {
    let name = local_ref_name(reference, "parameters")?;

    let target = document
        .spec
        .as_ref()
        .and_then(|spec| spec.components.as_ref())
        .and_then(|components| components.parameters.get(&name))
        .ok_or_else(|| ValidationError::MissingJsonPointerToken {
            token: name.clone(),
        })?;
    let raw_target = resolve_json_pointer(&document.raw, reference).ok();

    resolve_parameter(document, target, raw_target, context)
}

pub(super) fn resolve_request_body<'a>(
    document: &'a Document,
    request_body: &'a ObjectOrReference<OasRequestBody>,
    raw_request_body: Option<&'a Value>,
    context: &str,
) -> Result<(&'a OasRequestBody, Option<&'a Value>), ValidationError> {
    match request_body {
        ObjectOrReference::Object(request_body) => Ok((request_body, raw_request_body)),
        ObjectOrReference::Ref { ref_path, .. } => {
            reject_raw_ref_siblings(raw_request_body, context, false)?;
            let reference = ref_path.clone();
            resolve_request_body_ref(document, ref_path, context).map_err(|source| {
                ValidationError::ResolveReference {
                    reference,
                    context: context.to_owned(),
                    source: Box::new(source),
                }
            })
        }
    }
}

fn resolve_request_body_ref<'a>(
    document: &'a Document,
    reference: &str,
    context: &str,
) -> Result<(&'a OasRequestBody, Option<&'a Value>), ValidationError> {
    let name = local_ref_name(reference, "requestBodies")?;

    let target = document
        .spec
        .as_ref()
        .and_then(|spec| spec.components.as_ref())
        .and_then(|components| components.request_bodies.get(&name))
        .ok_or_else(|| ValidationError::MissingJsonPointerToken {
            token: name.clone(),
        })?;

    let raw_target = resolve_json_pointer(&document.raw, reference).ok();

    resolve_request_body(document, target, raw_target, context)
}

pub(super) fn resolve_response<'a>(
    document: &'a Document,
    response: &'a ObjectOrReference<OasResponse>,
    raw_response: Option<&'a Value>,
    context: &str,
) -> Result<(&'a OasResponse, Option<&'a Value>), ValidationError> {
    match response {
        ObjectOrReference::Object(response) => Ok((response, raw_response)),
        ObjectOrReference::Ref { ref_path, .. } => {
            reject_raw_ref_siblings(raw_response, context, false)?;
            let reference = ref_path.clone();
            resolve_response_ref(document, ref_path, context).map_err(|source| {
                ValidationError::ResolveReference {
                    reference,
                    context: context.to_owned(),
                    source: Box::new(source),
                }
            })
        }
    }
}

fn resolve_response_ref<'a>(
    document: &'a Document,
    reference: &str,
    context: &str,
) -> Result<(&'a OasResponse, Option<&'a Value>), ValidationError> {
    let name = local_ref_name(reference, "responses")?;

    let target = document
        .spec
        .as_ref()
        .and_then(|spec| spec.components.as_ref())
        .and_then(|components| components.responses.get(&name))
        .ok_or_else(|| ValidationError::MissingJsonPointerToken {
            token: name.clone(),
        })?;

    let raw_target = resolve_json_pointer(&document.raw, reference).ok();

    resolve_response(document, target, raw_target, context)
}

fn resolve_json_pointer<'a>(
    document: &'a Value,
    reference: &str,
) -> Result<&'a Value, ValidationError> {
    let Some(pointer) = reference.strip_prefix('#') else {
        return Err(ValidationError::NonLocalReference);
    };

    if pointer.is_empty() {
        return Ok(document);
    }

    if !pointer.starts_with('/') {
        return Err(ValidationError::InvalidLocalReference);
    }

    let mut current = document;
    for token in pointer[1..].split('/') {
        let token = json_pointer_unescape(token);
        current = current
            .as_object()
            .and_then(|object| object.get(&token))
            .ok_or_else(|| ValidationError::MissingJsonPointerToken {
                token: token.clone(),
            })?;
    }

    Ok(current)
}

fn local_ref_name(reference: &str, section: &'static str) -> Result<String, ValidationError> {
    let prefix = format!("#/components/{section}/");
    let Some(name) = reference.strip_prefix(&prefix) else {
        return Err(ValidationError::InvalidComponentReference {
            reference: reference.to_owned(),
            section,
        });
    };
    Ok(json_pointer_unescape(name))
}

fn json_pointer_unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}
