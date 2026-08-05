use std::collections::BTreeMap;

use oas3::{
    Map as OasMap,
    spec::{MediaType as OasMediaType, ObjectSchema as OasObjectSchema, SpecificationExtensions},
};
use serde::Deserialize;

use crate::error::ValidationError;
use crate::model::{IntegerType, ParseAs};

pub(super) fn optional_description(description: &Option<String>) -> Option<String> {
    description
        .as_deref()
        .filter(|description| !description.trim().is_empty())
        .map(str::to_owned)
}

/// Typed `x-satay` schema extension options deserialized via [`SpecificationExtensions`].
///
/// Satay codegen policy is layered on top of this raw parse: cross-field rules
/// (parse-as + integer-type compatibility, none-if applicability) live in
/// `parse/validate/satay.rs`. This struct only mirrors the wire shape; unknown
/// keys are rejected via `deny_unknown_fields`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct SataySchemaOptions {
    pub(crate) parse_as: Option<SatayParseAsWire>,
    pub(crate) integer_type: Option<SatayIntegerTypeWire>,
    pub(crate) treat_error_as_none: Option<bool>,
    pub(crate) none_if: Option<Vec<String>>,
    pub(crate) enum_variants: Option<BTreeMap<String, String>>,
}

impl SataySchemaOptions {
    pub(crate) fn parse_as(&self) -> Option<ParseAs> {
        self.parse_as.map(SatayParseAsWire::into_parse_as)
    }

    pub(crate) fn integer_type(&self) -> Option<IntegerType> {
        match self.integer_type {
            Some(SatayIntegerTypeWire::Auto) => None,
            Some(wire) => Some(wire.into_integer_type()),
            None => None,
        }
    }
}

/// Wire values for `x-satay.parse-as`. Mirrors the strings accepted by the
/// `parse-as` field's wire contract so unrecognized values surface as
/// deserialization errors instead of a separate validation variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SatayParseAsWire {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Bool,
    Date,
    NaiveDatetime,
    OffsetDatetime,
    Time,
    IntegerRange,
    NumberRange,
}

impl SatayParseAsWire {
    fn into_parse_as(self) -> ParseAs {
        match self {
            Self::U8 => ParseAs::U8,
            Self::U16 => ParseAs::U16,
            Self::U32 => ParseAs::U32,
            Self::U64 => ParseAs::U64,
            Self::I8 => ParseAs::I8,
            Self::I16 => ParseAs::I16,
            Self::I32 => ParseAs::I32,
            Self::I64 => ParseAs::I64,
            Self::F32 => ParseAs::F32,
            Self::F64 => ParseAs::F64,
            Self::Bool => ParseAs::Bool,
            Self::Date => ParseAs::Date,
            Self::NaiveDatetime => ParseAs::NaiveDateTime,
            Self::OffsetDatetime => ParseAs::OffsetDateTime,
            Self::Time => ParseAs::Time,
            Self::IntegerRange => ParseAs::IntegerRange,
            Self::NumberRange => ParseAs::NumberRange,
        }
    }
}

/// Wire values for `x-satay.integer-type`. `Auto` is the sentinel for
/// "infer from the schema's own integer format" and resolves to `None` at the
/// codegen layer; see [`SataySchemaOptions::integer_type`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SatayIntegerTypeWire {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    Auto,
}

impl SatayIntegerTypeWire {
    fn into_integer_type(self) -> IntegerType {
        match self {
            Self::U8 => IntegerType::U8,
            Self::U16 => IntegerType::U16,
            Self::U32 => IntegerType::U32,
            Self::U64 => IntegerType::U64,
            Self::I8 => IntegerType::I8,
            Self::I16 => IntegerType::I16,
            Self::I32 => IntegerType::I32,
            Self::I64 => IntegerType::I64,
            Self::Auto => {
                unreachable!("`auto` is handled before reaching `into_integer_type`")
            }
        }
    }
}

/// Reads the `x-satay` extension from an [`OasObjectSchema`] as a typed
/// [`SataySchemaOptions`]. Returns `None` when the extension is absent. The
/// `context` string is folded into any resulting [`ValidationError`] so the
/// caller's schema path (e.g. `property \`User.id\``) is preserved.
pub(crate) fn schema_options(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<Option<SataySchemaOptions>, ValidationError> {
    schema
        .extension_as::<SataySchemaOptions>("x-satay")
        .map_err(|source| ValidationError::extension_error(context, source))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_blank_descriptions() {
        assert_eq!(optional_description(&None), None);
        assert_eq!(optional_description(&Some(String::new())), None);
        assert_eq!(optional_description(&Some(" \n\t ".to_owned())), None);
        assert_eq!(
            optional_description(&Some("  useful text  ".to_owned())),
            Some("  useful text  ".to_owned())
        );
    }

    #[test]
    fn matches_json_media_types_case_insensitively() {
        assert!(is_json_media_type("application/json"));
        assert!(is_json_media_type("Application/JSON; charset=utf-8"));
        assert!(is_json_media_type("application/vnd.satay.user+json"));
        assert!(is_json_media_type("application/problem+JSON"));
        assert!(!is_json_media_type("text/json"));
        assert!(!is_json_media_type("application/xml"));
        assert!(!is_json_media_type("not-a-media-type"));
    }

    #[test]
    fn selects_explicit_json_before_suffix_json_media_type() {
        let mut content = OasMap::new();
        content.insert(
            "application/vnd.satay.user+json".to_owned(),
            OasMediaType::default(),
        );
        content.insert("application/json".to_owned(), OasMediaType::default());

        let (media_type, _) = json_media_type(&content).expect("json media type");
        assert_eq!(media_type, "application/json");
    }

    #[test]
    fn selects_first_suffix_json_media_type_when_exact_json_is_absent() {
        let mut content = OasMap::new();
        content.insert("application/xml".to_owned(), OasMediaType::default());
        content.insert(
            "application/vnd.satay.user+json; charset=utf-8".to_owned(),
            OasMediaType::default(),
        );

        let (media_type, _) = json_media_type(&content).expect("json media type");
        assert_eq!(media_type, "application/vnd.satay.user+json; charset=utf-8");
    }
}
