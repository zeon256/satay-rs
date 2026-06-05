use oas3::spec::Spec as OasSpec;

use std::borrow::Cow;

use crate::error::{ParseError, ValidationError};
use crate::model::Api;
use tracing::debug;

mod helpers;
mod lower;
mod reference;
mod registry;
mod resolve;
mod satay;
#[cfg(test)]
mod tests;
mod validate;

#[derive(Debug)]
pub(crate) struct Document {
    spec: OasSpec,
}

pub(crate) fn parse_api(document: &Document) -> Result<Api, ValidationError> {
    debug!("parsing API from document");

    let resolved = resolve::resolve_document(document)?;
    let validated = validate::validate_document(resolved)?;
    lower::lower_document(&validated)
}

pub(crate) fn parse_document(spec: &str) -> Result<Document, ParseError> {
    let normalized = normalize_oversized_yaml_schema_bounds(spec);
    let spec = oas3::from_yaml(normalized.as_ref())?;

    Ok(Document { spec })
}

fn normalize_oversized_yaml_schema_bounds(spec: &str) -> Cow<'_, str> {
    let mut normalized = None;
    let mut copied = 0;

    for line in spec.split_inclusive('\n') {
        let start = copied;
        copied += line.len();

        if let Some(line) = normalize_oversized_yaml_schema_bound_line(line) {
            let normalized = normalized.get_or_insert_with(|| {
                let mut output = String::with_capacity(spec.len() + 2);
                output.push_str(&spec[..start]);
                output
            });
            normalized.push_str(&line);
        } else if let Some(normalized) = &mut normalized {
            normalized.push_str(line);
        }
    }

    normalized.map_or(Cow::Borrowed(spec), Cow::Owned)
}

fn normalize_oversized_yaml_schema_bound_line(line: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
    ];

    let trimmed = line.trim_start();
    let keyword = KEYWORDS
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword)?.strip_prefix(':'))?;

    let value_start = keyword.len() - keyword.trim_start().len();
    let value = &keyword[value_start..];
    let value_len = value
        .find(|character: char| character.is_whitespace() || character == '#')
        .unwrap_or(value.len());
    let value = &value[..value_len];

    if !is_oversized_yaml_integer(value) {
        return None;
    }

    let value_start = line.len() - keyword.len() + value_start;
    let value_end = value_start + value.len();
    let mut normalized = String::with_capacity(line.len() + 2);
    normalized.push_str(&line[..value_end]);
    normalized.push_str(".0");
    normalized.push_str(&line[value_end..]);
    Some(normalized)
}

fn is_oversized_yaml_integer(value: &str) -> bool {
    if let Ok(value) = value.parse::<i128>() {
        return value < i128::from(i64::MIN) || value > i128::from(u64::MAX);
    }

    value
        .strip_prefix('+')
        .unwrap_or(value)
        .parse::<u128>()
        .is_ok_and(|value| value > u128::from(u64::MAX))
}
