use oas3::{
    Map as OasMap,
    spec::{
        MediaType as OasMediaType, ObjectOrReference, ObjectSchema as OasObjectSchema,
        Schema as OasSchema,
    },
};

use crate::error::ValidationError;

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
            ObjectOrReference::Ref { description, .. } => optional_description(description),
        },
    }
}

pub(super) fn satay_object<'a>(
    schema: &'a OasObjectSchema,
    context: &str,
) -> Result<Option<&'a serde_json::Map<String, serde_json::Value>>, ValidationError> {
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
