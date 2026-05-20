use oas3::{
    Map as OasMap,
    spec::{
        MediaType as OasMediaType, ObjectOrReference, ObjectSchema as OasObjectSchema,
        Schema as OasSchema,
    },
};
use serde_json::{Map, Value};

use super::Document;
use crate::error::ValidationError;

pub(super) fn object<'a>(
    value: &'a Value,
    context: &str,
) -> Result<&'a Map<String, Value>, ValidationError> {
    value
        .as_object()
        .ok_or_else(|| ValidationError::ExpectedObject {
            context: context.to_owned(),
        })
}

pub(super) fn optional_object<'a>(
    object: &'a Map<String, Value>,
    field: &'static str,
    context: &str,
) -> Result<Option<&'a Map<String, Value>>, ValidationError> {
    match object.get(field) {
        Some(value) => {
            value
                .as_object()
                .map(Some)
                .ok_or_else(|| ValidationError::ExpectedObjectField {
                    context: context.to_owned(),
                    field,
                })
        }
        None => Ok(None),
    }
}

pub(super) fn raw_components_map<'a>(
    document: &'a Document,
    field: &'static str,
) -> Option<&'a Map<String, Value>> {
    object(&document.raw, "OpenAPI document")
        .ok()
        .and_then(|root| root.get("components"))
        .and_then(Value::as_object)
        .and_then(|components| components.get(field))
        .and_then(Value::as_object)
}

pub(super) fn raw_field<'a>(raw: Option<&'a Value>, field: &str) -> Option<&'a Value> {
    raw.and_then(Value::as_object)
        .and_then(|object| object.get(field))
}

pub(super) fn optional_description(description: &Option<String>) -> Option<String> {
    description
        .as_deref()
        .filter(|description| !description.trim().is_empty())
        .map(str::to_owned)
}

pub(super) fn schema_description(schema: &OasSchema) -> Option<String> {
    match schema {
        OasSchema::Boolean(_) => None,
        OasSchema::Object(object) => match object.as_ref() {
            ObjectOrReference::Object(schema) => optional_description(&schema.description),
            ObjectOrReference::Ref { .. } => None,
        },
    }
}

pub(super) fn satay_object<'a>(
    schema: &'a OasObjectSchema,
    context: &str,
) -> Result<Option<&'a Map<String, Value>>, ValidationError> {
    let Some(value) = schema.extensions.get("satay") else {
        return Ok(None);
    };
    value
        .as_object()
        .map(Some)
        .ok_or_else(|| ValidationError::ExpectedObjectField {
            context: context.to_owned(),
            field: "x-satay",
        })
}

pub(super) fn required_str<'a>(
    object: &'a Map<String, Value>,
    field: &'static str,
    context: &str,
) -> Result<&'a str, ValidationError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| ValidationError::MissingStringField {
            context: context.to_owned(),
            field,
        })
}

pub(super) fn json_media_type(
    content: &OasMap<String, OasMediaType>,
) -> Option<(&str, &OasMediaType)> {
    content
        .get("application/json")
        .map(|value| ("application/json", value))
        .or_else(|| {
            content
                .iter()
                .find(|(media_type, _)| is_json_media_type(media_type))
                .map(|(media_type, value)| (media_type.as_str(), value))
        })
}

pub(super) fn is_json_media_type(value: &str) -> bool {
    let media_type = value.split(';').next().unwrap_or(value).trim();

    if media_type.eq_ignore_ascii_case("application/json") {
        return true;
    }

    let Some((_, subtype)) = media_type.rsplit_once('/') else {
        return false;
    };

    ends_with_ignore_ascii_case(subtype, "+json")
}

fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    let value = value.as_bytes();
    let suffix = suffix.as_bytes();

    value.len() >= suffix.len() && value[value.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}
