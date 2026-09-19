/// Stable source-level operation label used by diagnostics and name lowering.
pub(super) fn inferred_operation_id(method: &str, path: &str) -> String {
    let mut parts = vec![method.to_owned()];
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
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

use oas3::{Map as OasMap, spec::MediaType as OasMediaType};

pub(super) fn property_context(context: &str, name: &str) -> String {
    let parent = context
        .strip_prefix("schema `")
        .and_then(|value| value.strip_suffix('`'))
        .unwrap_or(context);
    format!("property `{parent}.{name}`")
}

pub(super) fn optional_description(description: &Option<String>) -> Option<String> {
    description
        .as_deref()
        .filter(|description| !description.trim().is_empty())
        .map(str::to_owned)
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

use crate::error::ValidationError;
use serde_json::Number;

pub(in crate::parse) fn json_integer(
    value: &Number,
    context: &str,
) -> Result<i128, ValidationError> {
    if let Some(value) = value.as_i64() {
        return Ok(i128::from(value));
    }
    if let Some(value) = value.as_u64() {
        return Ok(i128::from(value));
    }
    let Some(value) = value.as_f64() else {
        return Err(ValidationError::ExpectedInteger {
            context: context.to_owned(),
        });
    };
    if !value.is_finite() || value.fract() != 0.0 {
        return Err(ValidationError::ExpectedInteger {
            context: context.to_owned(),
        });
    }
    Ok(value as i128)
}

pub(in crate::parse) fn reject_keyword(
    present: bool,
    keyword: &'static str,
    context: &str,
) -> Result<(), ValidationError> {
    if present {
        return Err(ValidationError::UnsupportedKeyword {
            context: context.to_owned(),
            keyword: keyword.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
