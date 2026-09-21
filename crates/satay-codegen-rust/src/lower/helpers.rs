//! Rust-lowering-local copies of small pure helpers shared with the frontend.
//!
//! The frontend keeps its own copies in `crate::parse::helpers` for the
//! parser-facing callers; these are the backend halves with the backend
//! validation error type.

use serde_json::Number;

use super::error::ValidationError;

/// Stable source-level operation label used by name lowering.
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

pub(super) fn property_context(context: &str, name: &str) -> String {
    let parent = context
        .strip_prefix("schema `")
        .and_then(|value| value.strip_suffix('`'))
        .unwrap_or(context);
    format!("property `{parent}.{name}`")
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

pub(super) fn json_integer(value: &Number, context: &str) -> Result<i128, ValidationError> {
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

pub(super) fn reject_keyword(
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
