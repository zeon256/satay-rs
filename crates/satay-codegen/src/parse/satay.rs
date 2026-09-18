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
mod tests {
    use oas3::spec::{ObjectSchema as OasObjectSchema, Operation as OasOperation};
    use serde_json::{Value as JsonValue, json};

    use super::*;

    fn schema_with_satay(value: JsonValue) -> OasObjectSchema {
        let mut schema = OasObjectSchema::default();
        schema.extensions.insert("satay".to_owned(), value);
        schema
    }

    fn operation_with_satay(value: JsonValue) -> OasOperation {
        let mut operation = OasOperation::default();
        operation.extensions.insert("satay".to_owned(), value);
        operation
    }

    fn invalid_extension_path<T>(result: Result<T, ValidationError>) -> String {
        match result {
            Err(ValidationError::InvalidExtension { path, .. }) => path,
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("extension must be rejected"),
        }
    }

    #[test]
    fn reads_all_schema_options_from_the_central_wire_type() {
        let schema = schema_with_satay(json!({
            "parse-as": "u32",
            "integer-type": "u16",
            "treat-error-as-none": true,
            "none-if": ["", "-"],
            "true-values": ["Y", "Yes"],
            "false-values": ["N", "No"],
            "unknown-as": false,
            "enum-variants": { "A": "Available" },
            "ignore": true,
            "identifier": "request-id",
        }));

        let options = schema_options(&schema, "property `Status.value`")
            .expect("valid extension")
            .expect("present extension");

        assert_eq!(options.parse_as, Some(SatayParseAsWire::U32));
        assert_eq!(options.integer_type, Some(SatayIntegerTypeWire::U16));
        assert_eq!(options.treat_error_as_none, Some(true));
        assert_eq!(options.none_if, Some(vec![String::new(), "-".to_owned()]));
        assert_eq!(
            options.true_values,
            Some(vec!["Y".to_owned(), "Yes".to_owned()])
        );
        assert_eq!(
            options.false_values,
            Some(vec!["N".to_owned(), "No".to_owned()])
        );
        assert_eq!(options.unknown_as, Some(false));
        assert_eq!(options.ignore, Some(true));
        assert_eq!(
            options.identifier.as_ref().map(SatayIdentifier::words),
            Some(["request".to_owned(), "id".to_owned()].as_slice())
        );
        assert_eq!(
            options.enum_variants,
            Some(BTreeMap::from([("A".to_owned(), "Available".to_owned())]))
        );
    }

    #[test]
    fn preserves_auto_integer_type_wire_before_resolving_it() {
        let auto_schema = schema_with_satay(json!({ "integer-type": "auto" }));
        let auto_options = schema_options(&auto_schema, "schema `Count`")
            .expect("valid extension")
            .expect("present extension");

        assert_eq!(auto_options.integer_type, Some(SatayIntegerTypeWire::Auto));

        let absent_schema = schema_with_satay(json!({}));
        let absent_options = schema_options(&absent_schema, "schema `Count`")
            .expect("valid extension")
            .expect("present extension");
        assert_eq!(absent_options.integer_type, None);
    }

    #[test]
    fn missing_schema_and_operation_extensions_return_none() {
        assert!(
            schema_options(&OasObjectSchema::default(), "schema `Status`")
                .expect("missing extension is valid")
                .is_none()
        );
        assert!(
            operation_options(&OasOperation::default(), "operation `status`")
                .expect("missing extension is valid")
                .is_none()
        );
    }

    #[test]
    fn reads_operation_and_nested_output_options() {
        let operation = operation_with_satay(json!({
            "skip": true,
            "output": {
                "unwrap-field": "value",
                "map-field": "Link",
            },
        }));

        let options = operation_options(&operation, "operation `links`")
            .expect("valid extension")
            .expect("present extension");
        let output = options.output.expect("output options");

        assert!(options.skip);
        assert_eq!(output.unwrap_field.as_str(), "value");
        assert_eq!(
            output.map_field.as_ref().map(SatayFieldName::as_str),
            Some("Link")
        );
    }

    #[test]
    fn rejects_unknown_schema_and_operation_fields_with_precise_paths() {
        let schema = schema_with_satay(json!({ "unknown": true }));
        assert_eq!(
            invalid_extension_path(schema_options(&schema, "schema `Status`")),
            "x-satay.unknown"
        );

        let operation = operation_with_satay(json!({ "unknown": true }));
        assert_eq!(
            invalid_extension_path(operation_options(&operation, "operation `status`")),
            "x-satay.unknown"
        );
    }

    #[test]
    fn rejects_invalid_schema_values_with_precise_paths() {
        for (value, expected_path) in [
            (json!({ "parse-as": "uuid" }), "x-satay.parse-as"),
            (json!({ "ignore": "yes" }), "x-satay.ignore"),
            (json!({ "identifier": "request_id" }), "x-satay.identifier"),
            (json!({ "identifier": 7 }), "x-satay.identifier"),
            (json!({ "true-values": [true] }), "x-satay.true-values[0]"),
            (json!({ "unknown-as": "false" }), "x-satay.unknown-as"),
        ] {
            let schema = schema_with_satay(value);
            assert_eq!(
                invalid_extension_path(schema_options(&schema, "schema `Status`")),
                expected_path
            );
        }
    }

    #[test]
    fn rejects_invalid_and_unknown_output_fields_with_precise_paths() {
        for (output, expected_path) in [
            (json!({ "unwrap-field": "" }), "x-satay.output.unwrap-field"),
            (
                json!({ "unwrap-field": "value", "map-field": "" }),
                "x-satay.output.map-field",
            ),
            (
                json!({ "unwrap-field": "value", "unknown": true }),
                "x-satay.output.unknown",
            ),
        ] {
            let operation = operation_with_satay(json!({ "output": output }));
            assert_eq!(
                invalid_extension_path(operation_options(&operation, "operation `status`")),
                expected_path
            );
        }
    }
}
