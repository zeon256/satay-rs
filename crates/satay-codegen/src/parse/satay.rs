use std::collections::{BTreeMap, BTreeSet};

use oas3::spec::{
    ObjectSchema as OasObjectSchema, Operation as OasOperation, SchemaType as OasSchemaType,
    SpecificationExtensions,
};
use serde::Deserialize;

use super::reference::schema_type_wire;
use super::validate::constraint::parse_integer_type;
use crate::error::ValidationError;
use crate::ident::variant_ident;
use crate::model::{IntegerType, ParseAs, RangeScalar};

/// Typed schema-level `x-satay` wire options.
///
/// Compatibility between these fields and the surrounding OpenAPI schema is
/// validated in `parse/validate/satay.rs`. This type only defines the wire
/// contract and rejects unknown fields.
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

pub(super) fn parse_satay_enum_variants(
    options: &SataySchemaOptions,
    context: &str,
    enum_values: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>, ValidationError> {
    let Some(mappings) = options.enum_variants.as_ref() else {
        return Ok(BTreeMap::new());
    };

    let mut explicit = BTreeMap::new();
    let mut explicit_names = BTreeSet::new();

    for (wire_name, rust_name) in mappings {
        if !enum_values.contains(wire_name) {
            return Err(ValidationError::UnknownSatayEnumVariantValue {
                context: context.to_owned(),
                wire_name: wire_name.clone(),
            });
        }

        let rust_name = variant_ident(rust_name);
        if !explicit_names.insert(rust_name.clone()) {
            return Err(ValidationError::DuplicateSatayEnumVariantName {
                context: context.to_owned(),
                rust_name,
            });
        }
        explicit.insert(wire_name.clone(), rust_name);
    }

    Ok(explicit)
}

pub(super) fn parse_satay_parse_as(options: &SataySchemaOptions) -> Option<ParseAs> {
    options.parse_as()
}

pub(super) fn parse_satay_integer_type(options: &SataySchemaOptions) -> Option<IntegerType> {
    options.integer_type()
}

pub(super) fn validate_satay_integer_type(
    schema_type: Option<OasSchemaType>,
    parse_as: Option<ParseAs>,
    integer_type: Option<IntegerType>,
    context: &str,
) -> Result<(), ValidationError> {
    let Some(integer_type) = integer_type else {
        return Ok(());
    };

    let allowed = schema_type == Some(OasSchemaType::Integer)
        || matches!(
            (schema_type, parse_as),
            (Some(OasSchemaType::String), Some(ParseAs::IntegerRange))
        );

    if allowed {
        return Ok(());
    }

    Err(ValidationError::SatayIntegerTypeRequiresInteger {
        context: context.to_owned(),
        integer_type: satay_integer_type_wire(integer_type).to_owned(),
        kind: schema_type
            .map(schema_type_wire)
            .unwrap_or("missing")
            .to_owned(),
    })
}

pub(super) fn parse_range_scalar(
    schema: &OasObjectSchema,
    parse_as: ParseAs,
    integer_type: Option<IntegerType>,
    context: &str,
) -> Result<RangeScalar, ValidationError> {
    match parse_as {
        ParseAs::IntegerRange => Ok(RangeScalar::Integer(parse_integer_type(
            schema,
            context,
            integer_type,
        )?)),
        ParseAs::NumberRange => match schema.format.as_deref() {
            Some("float") => Ok(RangeScalar::F32),
            Some("double") | None => Ok(RangeScalar::F64),
            Some(format) => Err(ValidationError::UnsupportedNumberFormat {
                context: context.to_owned(),
                format: format.to_owned(),
            }),
        },
        _ => unreachable!("range scalar requires a range parse-as value"),
    }
}

pub(super) fn satay_parse_as_wire(parse_as: ParseAs) -> &'static str {
    match parse_as {
        ParseAs::U8 => "u8",
        ParseAs::U16 => "u16",
        ParseAs::U32 => "u32",
        ParseAs::U64 => "u64",
        ParseAs::I8 => "i8",
        ParseAs::I16 => "i16",
        ParseAs::I32 => "i32",
        ParseAs::I64 => "i64",
        ParseAs::F32 => "f32",
        ParseAs::F64 => "f64",
        ParseAs::Bool => "bool",
        ParseAs::Date => "date",
        ParseAs::NaiveDateTime => "naive-datetime",
        ParseAs::OffsetDateTime => "offset-datetime",
        ParseAs::UnixTime => "unixtime",
        ParseAs::Time => "time",
        ParseAs::IntegerRange => "integer-range",
        ParseAs::NumberRange => "number-range",
    }
}

fn satay_integer_type_wire(integer_type: IntegerType) -> &'static str {
    match integer_type {
        IntegerType::U8 => "u8",
        IntegerType::U16 => "u16",
        IntegerType::U32 => "u32",
        IntegerType::U64 => "u64",
        IntegerType::I8 => "i8",
        IntegerType::I16 => "i16",
        IntegerType::I32 => "i32",
        IntegerType::I64 => "i64",
    }
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
            "enum-variants": { "A": "Available" },
        }));

        let options = schema_options(&schema, "property `Status.value`")
            .expect("valid extension")
            .expect("present extension");

        assert_eq!(options.parse_as(), Some(ParseAs::U32));
        assert_eq!(options.integer_type(), Some(IntegerType::U16));
        assert_eq!(options.treat_error_as_none, Some(true));
        assert_eq!(options.none_if, Some(vec![String::new(), "-".to_owned()]));
        assert_eq!(
            options.enum_variants,
            Some(BTreeMap::from([("A".to_owned(), "Available".to_owned())]))
        );
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
        let schema = schema_with_satay(json!({ "parse-as": "uuid" }));
        assert_eq!(
            invalid_extension_path(schema_options(&schema, "schema `Status`")),
            "x-satay.parse-as"
        );
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
