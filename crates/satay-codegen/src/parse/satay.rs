use std::collections::BTreeMap;

use oas3::spec::{
    ObjectSchema as OasObjectSchema, Operation as OasOperation, SpecificationExtensions,
};
use serde::Deserialize;

use crate::error::ValidationError;

/// Typed schema-level `x-satay` wire options.
///
/// Compatibility between these fields and the surrounding OpenAPI schema is
/// validated during semantic normalization. This type only defines the wire
/// contract and rejects unknown fields.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct SataySchemaOptions {
    pub(crate) parse_as: Option<SatayParseAsWire>,
    pub(crate) target: Option<SatayCoordinateTarget>,
    pub(crate) fields: Option<Vec<SatayFieldName>>,
    pub(crate) delimiter: Option<String>,
    pub(crate) integer_type: Option<SatayIntegerTypeWire>,
    pub(crate) treat_error_as_none: Option<bool>,
    pub(crate) none_if: Option<Vec<String>>,
    pub(crate) true_values: Option<Vec<String>>,
    pub(crate) false_values: Option<Vec<String>>,
    pub(crate) unknown_as: Option<bool>,
    pub(crate) enum_variants: Option<BTreeMap<String, String>>,
    pub(crate) ignore: Option<bool>,
    pub(crate) identifier: Option<SatayIdentifier>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SatayCoordinateTarget {
    #[serde(rename = "$ref")]
    pub(crate) reference: String,
}

/// Extension references are not included in the OAS schema reference traversal.
pub(super) fn coordinate_target_reference(schema: &OasObjectSchema) -> Option<&str> {
    schema
        .extensions
        .get("satay")?
        .get("target")?
        .get("$ref")?
        .as_str()
}

/// A target-neutral property identifier represented as canonical words.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct SatayIdentifier(Vec<String>);

impl SatayIdentifier {
    pub(crate) fn words(&self) -> &[String] {
        &self.0
    }
}

impl TryFrom<String> for SatayIdentifier {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err("identifier must not be empty");
        }

        let words = value.split('-').collect::<Vec<_>>();
        if words.iter().any(|word| {
            word.is_empty()
                || !word
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        }) {
            return Err(
                "identifier must use lower kebab-case ASCII words (for example `request-id`)",
            );
        }

        Ok(Self(words.into_iter().map(str::to_owned).collect()))
    }
}

/// Typed operation-level `x-satay` wire options.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct SatayOperationOptions {
    #[serde(default)]
    pub(crate) skip: bool,
    pub(crate) output: Option<SatayOutputOptions>,
}

/// Wire selectors for projecting an operation response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct SatayOutputOptions {
    pub(crate) unwrap_field: SatayFieldName,
    pub(crate) map_field: Option<SatayFieldName>,
}

/// A non-empty JSON field selector used by Satay operation extensions.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct SatayFieldName(String);

impl SatayFieldName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SatayFieldName {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Err("field selector must not be empty")
        } else {
            Ok(Self(value))
        }
    }
}

/// Wire values for `x-satay.parse-as`. Mirrors the strings accepted by the
/// `parse-as` field's wire contract so unrecognized values surface as typed
/// deserialization errors.
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
    Coordinates,
}

/// Wire values for `x-satay.integer-type`. `Auto` asks codegen to infer the
/// integer type from the schema.
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

/// Reads a schema-level `x-satay` extension through the vendor-neutral typed
/// extension API.
pub(crate) fn schema_options(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<Option<SataySchemaOptions>, ValidationError> {
    schema
        .extension_as::<SataySchemaOptions>("x-satay")
        .map_err(|source| ValidationError::extension_error(context, source))
}

/// Reads an operation-level `x-satay` extension through the vendor-neutral
/// typed extension API.
pub(crate) fn operation_options(
    operation: &OasOperation,
    context: &str,
) -> Result<Option<SatayOperationOptions>, ValidationError> {
    operation
        .extension_as::<SatayOperationOptions>("x-satay")
        .map_err(|source| ValidationError::extension_error(context, source))
}

#[cfg(test)]
mod tests;
