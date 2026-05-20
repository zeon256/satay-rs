use oas3::spec::{
    ObjectOrReference, ObjectSchema as OasObjectSchema, Parameter as OasParameter,
    PathItem as OasPathItem, RequestBody as OasRequestBody, Response as OasResponse,
    Schema as OasSchema, SchemaType as OasSchemaType, SchemaTypeSet as OasSchemaTypeSet,
    SecurityScheme as OasSecurityScheme,
};

use super::Document;
use crate::error::ValidationError;
use crate::ident::type_ident;

pub(super) fn schema_ref<'a>(
    schema: &'a OasSchema,
    _context: &str,
) -> Result<Option<&'a str>, ValidationError> {
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
    context: &str,
) -> Result<(Option<OasSchemaType>, bool), ValidationError> {
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

pub(super) fn resolve_security_scheme<'a>(
    document: &'a Document,
    scheme: &'a ObjectOrReference<OasSecurityScheme>,
    context: &str,
) -> Result<&'a OasSecurityScheme, ValidationError> {
    match scheme {
        ObjectOrReference::Object(scheme) => Ok(scheme),
        ObjectOrReference::Ref { ref_path, .. } => {
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
        .components
        .as_ref()
        .and_then(|components| components.security_schemes.get(&name))
        .ok_or_else(|| ValidationError::MissingJsonPointerToken {
            token: name.clone(),
        })?;

    resolve_security_scheme(document, target, context)
}

pub(super) fn resolve_parameter<'a>(
    document: &'a Document,
    parameter: &'a ObjectOrReference<OasParameter>,
    context: &str,
) -> Result<&'a OasParameter, ValidationError> {
    match parameter {
        ObjectOrReference::Object(parameter) => Ok(parameter),
        ObjectOrReference::Ref { ref_path, .. } => {
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
) -> Result<&'a OasParameter, ValidationError> {
    let name = local_ref_name(reference, "parameters")?;

    let target = document
        .spec
        .components
        .as_ref()
        .and_then(|components| components.parameters.get(&name))
        .ok_or_else(|| ValidationError::MissingJsonPointerToken {
            token: name.clone(),
        })?;

    resolve_parameter(document, target, context)
}

pub(super) fn resolve_request_body<'a>(
    document: &'a Document,
    request_body: &'a ObjectOrReference<OasRequestBody>,
    context: &str,
) -> Result<&'a OasRequestBody, ValidationError> {
    match request_body {
        ObjectOrReference::Object(request_body) => Ok(request_body),
        ObjectOrReference::Ref { ref_path, .. } => {
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
) -> Result<&'a OasRequestBody, ValidationError> {
    let name = local_ref_name(reference, "requestBodies")?;

    let target = document
        .spec
        .components
        .as_ref()
        .and_then(|components| components.request_bodies.get(&name))
        .ok_or_else(|| ValidationError::MissingJsonPointerToken {
            token: name.clone(),
        })?;

    resolve_request_body(document, target, context)
}

pub(super) fn resolve_response<'a>(
    document: &'a Document,
    response: &'a ObjectOrReference<OasResponse>,
    context: &str,
) -> Result<&'a OasResponse, ValidationError> {
    match response {
        ObjectOrReference::Object(response) => Ok(response),
        ObjectOrReference::Ref { ref_path, .. } => {
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
) -> Result<&'a OasResponse, ValidationError> {
    let name = local_ref_name(reference, "responses")?;

    let target = document
        .spec
        .components
        .as_ref()
        .and_then(|components| components.responses.get(&name))
        .ok_or_else(|| ValidationError::MissingJsonPointerToken {
            token: name.clone(),
        })?;

    resolve_response(document, target, context)
}

pub(super) fn resolve_path_item<'a>(
    document: &'a Document,
    path_item: &'a OasPathItem,
    context: &str,
) -> Result<&'a OasPathItem, ValidationError> {
    let Some(reference) = path_item.reference.as_deref() else {
        return Ok(path_item);
    };

    let name = local_ref_name(reference, "pathItems").map_err(|source| {
        ValidationError::ResolveReference {
            reference: reference.to_owned(),
            context: context.to_owned(),
            source: Box::new(source),
        }
    })?;

    let target = document
        .spec
        .components
        .as_ref()
        .and_then(|components| components.path_items.get(&name))
        .ok_or_else(|| ValidationError::ResolveReference {
            reference: reference.to_owned(),
            context: context.to_owned(),
            source: Box::new(ValidationError::MissingJsonPointerToken {
                token: name.clone(),
            }),
        })?;

    resolve_path_item_reference(document, target, context, reference)
}

fn resolve_path_item_reference<'a>(
    document: &'a Document,
    path_item: &'a ObjectOrReference<OasPathItem>,
    context: &str,
    reference: &str,
) -> Result<&'a OasPathItem, ValidationError> {
    match path_item {
        ObjectOrReference::Object(path_item) => {
            resolve_path_item(document, path_item, context)
        }
        ObjectOrReference::Ref { ref_path, .. } => {
            let name = local_ref_name(ref_path, "pathItems").map_err(|source| {
                ValidationError::ResolveReference {
                    reference: reference.to_owned(),
                    context: context.to_owned(),
                    source: Box::new(source),
                }
            })?;
            let target = document
                .spec
                .components
                .as_ref()
                .and_then(|components| components.path_items.get(&name))
                .ok_or_else(|| ValidationError::ResolveReference {
                    reference: reference.to_owned(),
                    context: context.to_owned(),
                    source: Box::new(ValidationError::MissingJsonPointerToken { token: name }),
                })?;
            resolve_path_item_reference(document, target, context, reference)
        }
    }
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
