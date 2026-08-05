use std::collections::{BTreeMap, BTreeSet};

use oas3::spec::{ObjectSchema as OasObjectSchema, SchemaType as OasSchemaType};
use serde_json::Value as JsonValue;

use super::super::helpers::schema_options;
use super::super::reference::schema_type_wire;
use super::super::satay::{
    parse_range_scalar, parse_satay_enum_variants, parse_satay_integer_type, parse_satay_parse_as,
    satay_parse_as_wire, validate_satay_integer_type,
};
use crate::error::ValidationError;
use crate::model::{IntegerType, ParseAs, RangeScalar};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidatedParseAs {
    ParsedString(ParseAs),
    ParsedInteger(ParseAs),
    Range(RangeScalar),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ValidatedSataySchema {
    pub(crate) parse_as: Option<ValidatedParseAs>,
    pub(crate) explicit_integer_type: Option<IntegerType>,
    pub(crate) enum_variants: BTreeMap<String, String>,
    pub(crate) treat_error_as_none: bool,
    pub(crate) none_if: Vec<String>,
}

pub(super) fn validate_component_enum_satay(
    schema: &OasObjectSchema,
    enum_values: &[JsonValue],
    context: &str,
) -> Result<ValidatedSataySchema, ValidationError> {
    Ok(ValidatedSataySchema {
        enum_variants: validate_enum_variants(schema, enum_values, context)?,
        ..ValidatedSataySchema::default()
    })
}

pub(super) fn validate_type_satay(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    allow_treat_error_as_none: bool,
) -> Result<ValidatedSataySchema, ValidationError> {
    let options = schema_options(schema, context)?.unwrap_or_default();
    let parse_as = parse_satay_parse_as(&options);
    let explicit_integer_type = parse_satay_integer_type(&options);
    let none_if = match options.none_if.as_ref() {
        Some(values) if values.is_empty() => {
            return Err(ValidationError::EmptySatayNoneIf {
                context: context.to_owned(),
            });
        }
        Some(values) => values.clone(),
        None => vec![],
    };
    let treat_error_as_none =
        allow_treat_error_as_none && options.treat_error_as_none.unwrap_or(false);

    validate_satay_integer_type(schema_type, parse_as, explicit_integer_type, context)?;

    let parse_as = if let Some(parse_as) = parse_as {
        match (schema_type, parse_as) {
            (Some(OasSchemaType::String), ParseAs::IntegerRange | ParseAs::NumberRange) => {
                Some(ValidatedParseAs::Range(parse_range_scalar(
                    schema,
                    parse_as,
                    explicit_integer_type,
                    context,
                )?))
            }
            (Some(OasSchemaType::String), parse_as) => {
                Some(ValidatedParseAs::ParsedString(parse_as))
            }
            (Some(OasSchemaType::Integer), ParseAs::Bool) => {
                Some(ValidatedParseAs::ParsedInteger(parse_as))
            }
            _ => {
                return Err(ValidationError::SatayParseAsRequiresString {
                    context: context.to_owned(),
                    parse_as: satay_parse_as_wire(parse_as).to_owned(),
                    kind: schema_type
                        .map(schema_type_wire)
                        .unwrap_or("missing")
                        .to_owned(),
                });
            }
        }
    } else {
        None
    };

    if !none_if.is_empty() && !allow_treat_error_as_none {
        return Err(ValidationError::SatayNoneIfRequiresStructField {
            context: context.to_owned(),
        });
    }
    if !none_if.is_empty() && !matches!(parse_as, Some(ValidatedParseAs::ParsedString(_))) {
        return Err(ValidationError::SatayNoneIfRequiresParsedString {
            context: context.to_owned(),
        });
    }
    if !none_if.is_empty() && treat_error_as_none {
        return Err(ValidationError::ConflictingSatayNoneHandling {
            context: context.to_owned(),
        });
    }

    Ok(ValidatedSataySchema {
        parse_as,
        explicit_integer_type,
        treat_error_as_none,
        none_if,
        ..ValidatedSataySchema::default()
    })
}

pub(super) fn validate_type_enum_satay(
    schema: &OasObjectSchema,
    enum_values: &[JsonValue],
    context: &str,
) -> Result<BTreeMap<String, String>, ValidationError> {
    validate_enum_variants(schema, enum_values, context)
}

fn validate_enum_variants(
    schema: &OasObjectSchema,
    enum_values: &[JsonValue],
    context: &str,
) -> Result<BTreeMap<String, String>, ValidationError> {
    if enum_values.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut wire_names = BTreeSet::new();
    for value in enum_values {
        let Some(value) = value.as_str() else {
            return Ok(BTreeMap::new());
        };
        wire_names.insert(value.to_owned());
    }

    let options = schema_options(schema, context)?.unwrap_or_default();
    parse_satay_enum_variants(&options, context, &wire_names)
}

#[cfg(test)]
mod tests {
    use oas3::spec::ObjectSchema as OasObjectSchema;
    use serde_json::{Value, json};

    use super::*;

    fn schema_with_satay(satay: Value) -> OasObjectSchema {
        let mut schema = OasObjectSchema::default();
        schema.extensions.insert("satay".to_owned(), satay);
        schema
    }

    fn validation_error<T>(result: Result<T, ValidationError>) -> ValidationError {
        match result {
            Ok(_) => panic!("expected validation error"),
            Err(error) => error,
        }
    }

    #[test]
    fn validates_parse_as_for_string_schema() {
        let schema = schema_with_satay(json!({ "parse-as": "offset-datetime" }));

        let validated = validate_type_satay(
            &schema,
            Some(OasSchemaType::String),
            "Event.created_at",
            false,
        )
        .unwrap();

        assert_eq!(
            validated.parse_as,
            Some(ValidatedParseAs::ParsedString(ParseAs::OffsetDateTime))
        );
        assert_eq!(validated.explicit_integer_type, None);
        assert!(!validated.treat_error_as_none);
    }

    #[test]
    fn validates_parse_as_date_for_string_schema() {
        let schema = schema_with_satay(json!({ "parse-as": "date" }));

        let validated = validate_type_satay(
            &schema,
            Some(OasSchemaType::String),
            "parameter `date`",
            false,
        )
        .unwrap();

        assert_eq!(
            validated.parse_as,
            Some(ValidatedParseAs::ParsedString(ParseAs::Date))
        );
    }

    #[test]
    fn validates_parse_as_naive_datetime_for_string_schema() {
        let schema = schema_with_satay(json!({ "parse-as": "naive-datetime" }));

        let validated = validate_type_satay(
            &schema,
            Some(OasSchemaType::String),
            "parameter `date`",
            false,
        )
        .unwrap();

        assert_eq!(
            validated.parse_as,
            Some(ValidatedParseAs::ParsedString(ParseAs::NaiveDateTime))
        );
    }

    #[test]
    fn allows_bool_parse_as_for_integer_schema() {
        let schema = schema_with_satay(json!({ "parse-as": "bool" }));

        let validated =
            validate_type_satay(&schema, Some(OasSchemaType::Integer), "Flag.enabled", false)
                .unwrap();

        assert_eq!(
            validated.parse_as,
            Some(ValidatedParseAs::ParsedInteger(ParseAs::Bool))
        );
    }

    #[test]
    fn rejects_parse_as_for_unsupported_wire_schema() {
        let schema = schema_with_satay(json!({ "parse-as": "time" }));

        let error = validation_error(validate_type_satay(
            &schema,
            Some(OasSchemaType::Number),
            "Event.at",
            false,
        ));

        assert!(matches!(
            error,
            ValidationError::SatayParseAsRequiresString {
                context,
                parse_as,
                kind,
            } if context == "Event.at" && parse_as == "time" && kind == "number"
        ));
    }

    #[test]
    fn validates_integer_range_scalar_with_explicit_integer_type() {
        let schema = schema_with_satay(json!({
            "parse-as": "integer-range",
            "integer-type": "u16",
        }));

        let validated = validate_type_satay(
            &schema,
            Some(OasSchemaType::String),
            "RangeFilter.age",
            false,
        )
        .unwrap();

        assert_eq!(
            validated.parse_as,
            Some(ValidatedParseAs::Range(RangeScalar::Integer(
                IntegerType::U16
            )))
        );
        assert_eq!(validated.explicit_integer_type, Some(IntegerType::U16));
    }

    #[test]
    fn rejects_integer_type_for_plain_string_schema() {
        let schema = schema_with_satay(json!({ "integer-type": "i32" }));

        let error = validation_error(validate_type_satay(
            &schema,
            Some(OasSchemaType::String),
            "User.id",
            false,
        ));

        assert!(matches!(
            error,
            ValidationError::SatayIntegerTypeRequiresInteger {
                context,
                integer_type,
                kind,
            } if context == "User.id" && integer_type == "i32" && kind == "string"
        ));
    }

    #[test]
    fn validates_enum_variant_overrides() {
        let mut schema = schema_with_satay(json!({
            "enum-variants": {
                "in-progress": "InProgress",
                "done": "Done",
            }
        }));
        schema.enum_values = vec![json!("in-progress"), json!("done")];

        let variants =
            validate_type_enum_satay(&schema, &schema.enum_values, "Task.status").unwrap();

        assert_eq!(
            variants.get("in-progress").map(String::as_str),
            Some("InProgress")
        );
        assert_eq!(variants.get("done").map(String::as_str), Some("Done"));
        assert_eq!(variants.len(), 2);
    }

    #[test]
    fn rejects_unknown_enum_variant_override() {
        let mut schema = schema_with_satay(json!({
            "enum-variants": {
                "archived": "Archived",
            }
        }));
        schema.enum_values = vec![json!("active")];

        let error = validation_error(validate_type_enum_satay(
            &schema,
            &schema.enum_values,
            "Task.status",
        ));

        assert!(matches!(
            error,
            ValidationError::UnknownSatayEnumVariantValue { context, wire_name }
                if context == "Task.status" && wire_name == "archived"
        ));
    }

    #[test]
    fn validates_treat_error_as_none_only_when_allowed() {
        let schema = schema_with_satay(json!({ "treat-error-as-none": true }));

        let allowed =
            validate_type_satay(&schema, Some(OasSchemaType::String), "User.nickname", true)
                .unwrap();
        let ignored =
            validate_type_satay(&schema, Some(OasSchemaType::String), "User.nickname", false)
                .unwrap();

        assert!(allowed.treat_error_as_none);
        assert!(!ignored.treat_error_as_none);
    }

    #[test]
    fn rejects_non_boolean_treat_error_as_none() {
        let schema = schema_with_satay(json!({ "treat-error-as-none": "yes" }));

        let error = validation_error(validate_type_satay(
            &schema,
            Some(OasSchemaType::String),
            "User.nickname",
            true,
        ));

        assert!(matches!(
            error,
            ValidationError::InvalidExtension {
                context,
                path,
                source: _,
            } if context == "User.nickname" && path == "x-satay.treat-error-as-none"
        ));
    }
}
