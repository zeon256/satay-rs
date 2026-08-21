use std::collections::{BTreeMap, BTreeSet};

use oas3::spec::{ObjectSchema as OasObjectSchema, SchemaType as OasSchemaType};
use serde_json::Value as JsonValue;

use super::super::reference::schema_type_wire;
use super::super::satay::{
    SatayIdentifier, SataySchemaOptions, parse_range_scalar, parse_satay_enum_variants,
    parse_satay_parse_as, satay_parse_as_wire, schema_options, validate_satay_integer_type,
};
use super::constraint::parse_integer_type;
use super::{NonEmptySentinels, ValidatedFieldDecoding};
use crate::error::ValidationError;
use crate::model::{
    BoolStringMapping, BoolStringMappingError, IntegerType, ParseAs, RangeScalar, StringCodec,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValidatedTypeDirective {
    AsDeclared,
    Integer(IntegerType),
    ParsedString(StringCodec),
    ParsedIntegerBool,
    Range(RangeScalar),
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedSataySchema {
    pub(crate) directive: ValidatedTypeDirective,
    pub(super) property_options: Option<ValidatedPropertyOptions>,
}

#[derive(Debug, Clone)]
pub(super) struct ValidatedPropertyOptions {
    pub(super) field_decoding: ValidatedFieldDecoding,
    pub(super) ignore: bool,
    pub(super) identifier: Option<SatayIdentifier>,
}

impl ValidatedPropertyOptions {
    pub(super) fn strict() -> Self {
        Self {
            field_decoding: ValidatedFieldDecoding::Strict,
            ignore: false,
            identifier: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SatayValidationContext {
    Value,
    Property,
}

pub(super) fn reject_property_options_on_value_schema(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    let options = schema_options(schema, context)?.unwrap_or_default();
    reject_property_options_on_value(&options, context)
}

pub(super) fn validate_value_satay(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
) -> Result<ValidatedSataySchema, ValidationError> {
    validate_type_satay(schema, schema_type, context, SatayValidationContext::Value)
}

pub(super) fn validate_property_satay(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
) -> Result<ValidatedSataySchema, ValidationError> {
    validate_type_satay(
        schema,
        schema_type,
        context,
        SatayValidationContext::Property,
    )
}

fn validate_type_satay(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    validation_context: SatayValidationContext,
) -> Result<ValidatedSataySchema, ValidationError> {
    let options = schema_options(schema, context)?.unwrap_or_default();
    let parse_as = parse_satay_parse_as(&options);
    let integer_type_wire = options.integer_type_wire();
    let sentinels = options
        .none_if
        .as_ref()
        .map(|values| {
            NonEmptySentinels::new(values.clone()).map_err(|_| ValidationError::EmptySatayNoneIf {
                context: context.to_owned(),
            })
        })
        .transpose()?;
    let (treat_error_as_none, ignore, identifier) = match validation_context {
        SatayValidationContext::Value => {
            reject_property_options_on_value(&options, context)?;
            (false, false, None)
        }
        SatayValidationContext::Property => (
            options.treat_error_as_none.unwrap_or(false),
            options.ignore.unwrap_or(false),
            options.identifier.clone(),
        ),
    };
    validate_satay_integer_type(schema_type, parse_as, integer_type_wire, context)?;

    let directive = if let Some(parse_as) = parse_as {
        match (schema_type, parse_as) {
            (Some(OasSchemaType::String), ParseAs::IntegerRange | ParseAs::NumberRange) => {
                ValidatedTypeDirective::Range(parse_range_scalar(
                    schema,
                    parse_as,
                    options.integer_type(),
                    context,
                )?)
            }
            (Some(OasSchemaType::String), parse_as) => {
                ValidatedTypeDirective::ParsedString(StringCodec::Standard(parse_as))
            }
            (Some(OasSchemaType::Integer), ParseAs::Bool) => {
                ValidatedTypeDirective::ParsedIntegerBool
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
    } else if schema_type == Some(OasSchemaType::Integer) && integer_type_wire.is_some() {
        let integer_type = if schema.format.as_deref() == Some("unixtime") {
            // Preserve integer-type presence in the validated directive;
            // unixtime format conversion remains authoritative downstream.
            options.integer_type().unwrap_or(IntegerType::I64)
        } else {
            parse_integer_type(schema, context, options.integer_type())?
        };
        ValidatedTypeDirective::Integer(integer_type)
    } else {
        ValidatedTypeDirective::AsDeclared
    };

    if sentinels.is_some() && !matches!(&directive, ValidatedTypeDirective::ParsedString(_)) {
        return Err(ValidationError::SatayNoneIfRequiresParsedString {
            context: context.to_owned(),
        });
    }
    if sentinels.is_some() && treat_error_as_none {
        return Err(ValidationError::ConflictingSatayNoneHandling {
            context: context.to_owned(),
        });
    }

    let none_if = sentinels
        .as_ref()
        .map_or(&[][..], NonEmptySentinels::as_slice);
    let bool_string_mapping = validate_bool_string_mapping(&options, &directive, none_if, context)?;
    let directive = match (directive, bool_string_mapping) {
        (ValidatedTypeDirective::ParsedString(_), Some(mapping)) => {
            ValidatedTypeDirective::ParsedString(StringCodec::MappedBool(mapping))
        }
        (directive, None) => directive,
        _ => unreachable!("validated boolean mapping requires a parsed string"),
    };

    let field_decoding = match sentinels {
        Some(sentinels) => ValidatedFieldDecoding::Sentinel(sentinels),
        None if treat_error_as_none => ValidatedFieldDecoding::Lossy,
        None => ValidatedFieldDecoding::Strict,
    };

    let property_options = match validation_context {
        SatayValidationContext::Value => None,
        SatayValidationContext::Property => Some(ValidatedPropertyOptions {
            field_decoding,
            ignore,
            identifier,
        }),
    };

    Ok(ValidatedSataySchema {
        directive,
        property_options,
    })
}

fn validate_bool_string_mapping(
    options: &SataySchemaOptions,
    directive: &ValidatedTypeDirective,
    none_if: &[String],
    context: &str,
) -> Result<Option<BoolStringMapping>, ValidationError> {
    let configured = options.true_values.is_some()
        || options.false_values.is_some()
        || options.unknown_as.is_some();
    if !configured {
        return Ok(None);
    }
    if !matches!(
        directive,
        ValidatedTypeDirective::ParsedString(codec) if codec.parse_as() == ParseAs::Bool
    ) {
        return Err(ValidationError::SatayBoolMappingRequiresParsedStringBool {
            context: context.to_owned(),
        });
    }

    let (Some(true_values), Some(false_values)) =
        (options.true_values.as_ref(), options.false_values.as_ref())
    else {
        return Err(ValidationError::IncompleteSatayBoolMapping {
            context: context.to_owned(),
        });
    };
    let mapping = BoolStringMapping::try_new(
        true_values.clone(),
        false_values.clone(),
        options.unknown_as,
    )
    .map_err(|error| match error {
        BoolStringMappingError::EmptyTrueValues => ValidationError::EmptySatayBoolMapping {
            context: context.to_owned(),
            keyword: "true-values",
        },
        BoolStringMappingError::EmptyFalseValues => ValidationError::EmptySatayBoolMapping {
            context: context.to_owned(),
            keyword: "false-values",
        },
        BoolStringMappingError::OverlappingValue(value) => {
            ValidationError::OverlappingSatayBoolMapping {
                context: context.to_owned(),
                value,
            }
        }
    })?;

    if let Some(value) = none_if.iter().find(|value| {
        mapping.true_values().contains(value) || mapping.false_values().contains(value)
    }) {
        return Err(ValidationError::OverlappingSatayBoolMappingNoneIf {
            context: context.to_owned(),
            value: value.clone(),
        });
    }

    Ok(Some(mapping))
}

fn reject_property_options_on_value(
    options: &SataySchemaOptions,
    context: &str,
) -> Result<(), ValidationError> {
    if options.treat_error_as_none.is_some() {
        return Err(
            ValidationError::SatayTreatErrorAsNoneRequiresObjectProperty {
                context: context.to_owned(),
            },
        );
    }
    if options.none_if.is_some() {
        return Err(ValidationError::SatayNoneIfRequiresStructField {
            context: context.to_owned(),
        });
    }
    if options.ignore.is_some() {
        return Err(ValidationError::SatayIgnoreRequiresObjectProperty {
            context: context.to_owned(),
        });
    }
    if options.identifier.is_some() {
        return Err(ValidationError::SatayIdentifierRequiresObjectProperty {
            context: context.to_owned(),
        });
    }

    Ok(())
}

pub(super) fn reject_enum_variants_without_enum(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    let options = schema_options(schema, context)?.unwrap_or_default();
    if options.enum_variants.is_some() {
        return Err(ValidationError::SatayEnumVariantsRequireEnum {
            context: context.to_owned(),
        });
    }

    Ok(())
}

pub(super) fn validate_enum_satay(
    schema: &OasObjectSchema,
    enum_values: &[JsonValue],
    context: &str,
) -> Result<BTreeMap<String, String>, ValidationError> {
    let options = schema_options(schema, context)?.unwrap_or_default();
    if let Some(parse_as) = parse_satay_parse_as(&options) {
        return Err(ValidationError::SatayParseAsWithEnum {
            context: context.to_owned(),
            parse_as: satay_parse_as_wire(parse_as).to_owned(),
        });
    }

    let mut wire_names = BTreeSet::new();
    for value in enum_values {
        let Some(value) = value.as_str() else {
            return Ok(BTreeMap::new());
        };
        wire_names.insert(value.to_owned());
    }

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
    fn uses_as_declared_without_type_options() {
        let schema = schema_with_satay(json!({}));

        for schema_type in [
            Some(OasSchemaType::String),
            Some(OasSchemaType::Integer),
            Some(OasSchemaType::Number),
            Some(OasSchemaType::Boolean),
            None,
        ] {
            let validated = validate_value_satay(&schema, schema_type, "Value").unwrap();
            assert_eq!(validated.directive, ValidatedTypeDirective::AsDeclared);
        }
    }

    #[test]
    fn validates_parse_as_for_string_schema() {
        let schema = schema_with_satay(json!({ "parse-as": "offset-datetime" }));

        let validated =
            validate_value_satay(&schema, Some(OasSchemaType::String), "Event.created_at").unwrap();

        assert_eq!(
            validated.directive,
            ValidatedTypeDirective::ParsedString(StringCodec::Standard(ParseAs::OffsetDateTime))
        );
        assert!(validated.property_options.is_none());
    }

    #[test]
    fn validates_parse_as_date_for_string_schema() {
        let schema = schema_with_satay(json!({ "parse-as": "date" }));

        let validated =
            validate_value_satay(&schema, Some(OasSchemaType::String), "parameter `date`").unwrap();

        assert_eq!(
            validated.directive,
            ValidatedTypeDirective::ParsedString(StringCodec::Standard(ParseAs::Date))
        );
    }

    #[test]
    fn validates_parse_as_naive_datetime_for_string_schema() {
        let schema = schema_with_satay(json!({ "parse-as": "naive-datetime" }));

        let validated =
            validate_value_satay(&schema, Some(OasSchemaType::String), "parameter `date`").unwrap();

        assert_eq!(
            validated.directive,
            ValidatedTypeDirective::ParsedString(StringCodec::Standard(ParseAs::NaiveDateTime))
        );
    }

    #[test]
    fn allows_bool_parse_as_for_integer_schema() {
        let schema = schema_with_satay(json!({ "parse-as": "bool" }));

        let validated =
            validate_value_satay(&schema, Some(OasSchemaType::Integer), "Flag.enabled").unwrap();

        assert_eq!(
            validated.directive,
            ValidatedTypeDirective::ParsedIntegerBool
        );
    }

    #[test]
    fn rejects_integer_bool_with_exact_type() {
        let schema = schema_with_satay(json!({
            "parse-as": "bool",
            "integer-type": "i32",
        }));

        let error = validation_error(validate_value_satay(
            &schema,
            Some(OasSchemaType::Integer),
            "Flag.enabled",
        ));
        assert_eq!(
            error.to_string(),
            "Flag.enabled cannot combine x-satay.parse-as `bool` with x-satay.integer-type `i32`"
        );

        assert!(matches!(
            error,
            ValidationError::SatayParseAsBoolWithIntegerType {
                context,
                integer_type,
            } if context == "Flag.enabled" && integer_type == "i32"
        ));
    }

    #[test]
    fn rejects_integer_bool_with_auto_integer_type() {
        let schema = schema_with_satay(json!({
            "parse-as": "bool",
            "integer-type": "auto",
        }));

        let error = validation_error(validate_value_satay(
            &schema,
            Some(OasSchemaType::Integer),
            "Flag.enabled",
        ));

        assert!(matches!(
            error,
            ValidationError::SatayParseAsBoolWithIntegerType {
                context,
                integer_type,
            } if context == "Flag.enabled" && integer_type == "auto"
        ));
    }

    #[test]
    fn validates_complete_boolean_string_mapping() {
        let schema = schema_with_satay(json!({
            "parse-as": "bool",
            "true-values": ["Y", "Yes"],
            "false-values": ["N", "No"],
            "unknown-as": false,
        }));

        let validated =
            validate_property_satay(&schema, Some(OasSchemaType::String), "Flag.enabled").unwrap();

        assert_eq!(
            validated.directive,
            ValidatedTypeDirective::ParsedString(StringCodec::MappedBool(
                BoolStringMapping::try_new(
                    vec!["Y".to_owned(), "Yes".to_owned()],
                    vec!["N".to_owned(), "No".to_owned()],
                    Some(false),
                )
                .expect("boolean string mapping should be valid")
            ))
        );
    }

    #[test]
    fn rejects_incomplete_or_empty_boolean_string_mappings() {
        let incomplete = schema_with_satay(json!({
            "parse-as": "bool",
            "true-values": ["Y"],
        }));
        assert!(matches!(
            validation_error(validate_property_satay(
                &incomplete,
                Some(OasSchemaType::String),
                "Flag.enabled",
            )),
            ValidationError::IncompleteSatayBoolMapping { context }
                if context == "Flag.enabled"
        ));

        let empty = schema_with_satay(json!({
            "parse-as": "bool",
            "true-values": [],
            "false-values": ["N"],
        }));
        assert!(matches!(
            validation_error(validate_property_satay(
                &empty,
                Some(OasSchemaType::String),
                "Flag.enabled",
            )),
            ValidationError::EmptySatayBoolMapping {
                context,
                keyword: "true-values",
            } if context == "Flag.enabled"
        ));
    }

    #[test]
    fn rejects_overlapping_boolean_and_none_mappings() {
        let overlapping = schema_with_satay(json!({
            "parse-as": "bool",
            "true-values": ["Y", "same"],
            "false-values": ["N", "same"],
        }));
        assert!(matches!(
            validation_error(validate_property_satay(
                &overlapping,
                Some(OasSchemaType::String),
                "Flag.enabled",
            )),
            ValidationError::OverlappingSatayBoolMapping { context, value }
                if context == "Flag.enabled" && value == "same"
        ));

        let overlaps_none = schema_with_satay(json!({
            "parse-as": "bool",
            "true-values": ["Y"],
            "false-values": ["N"],
            "none-if": ["N"],
        }));
        assert!(matches!(
            validation_error(validate_property_satay(
                &overlaps_none,
                Some(OasSchemaType::String),
                "Flag.enabled",
            )),
            ValidationError::OverlappingSatayBoolMappingNoneIf { context, value }
                if context == "Flag.enabled" && value == "N"
        ));
    }

    #[test]
    fn allows_boolean_mappings_outside_fields_but_rejects_other_parsers() {
        let schema = schema_with_satay(json!({
            "parse-as": "bool",
            "true-values": ["Y"],
            "false-values": ["N"],
        }));
        let parameter =
            validate_value_satay(&schema, Some(OasSchemaType::String), "parameter `enabled`")
                .unwrap();
        assert_eq!(
            parameter.directive,
            ValidatedTypeDirective::ParsedString(StringCodec::MappedBool(
                BoolStringMapping::try_new(vec!["Y".to_owned()], vec!["N".to_owned()], None,)
                    .expect("boolean string mapping should be valid")
            ))
        );

        let wrong_parser = schema_with_satay(json!({
            "parse-as": "u8",
            "true-values": ["Y"],
            "false-values": ["N"],
        }));
        assert!(matches!(
            validation_error(validate_property_satay(
                &wrong_parser,
                Some(OasSchemaType::String),
                "Flag.enabled",
            )),
            ValidationError::SatayBoolMappingRequiresParsedStringBool { context }
                if context == "Flag.enabled"
        ));
    }

    #[test]
    fn rejects_parse_as_for_unsupported_wire_schema() {
        let schema = schema_with_satay(json!({ "parse-as": "time" }));

        let error = validation_error(validate_value_satay(
            &schema,
            Some(OasSchemaType::Number),
            "Event.at",
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
    fn validates_integer_range_scalar_with_exact_type() {
        let schema = schema_with_satay(json!({
            "parse-as": "integer-range",
            "integer-type": "u16",
        }));

        let validated =
            validate_value_satay(&schema, Some(OasSchemaType::String), "RangeFilter.age").unwrap();

        assert_eq!(
            validated.directive,
            ValidatedTypeDirective::Range(RangeScalar::Integer(IntegerType::U16))
        );
    }

    #[test]
    fn resolves_exact_integer_type_to_concrete_directive() {
        let schema = schema_with_satay(json!({ "integer-type": "u16" }));

        let validated =
            validate_value_satay(&schema, Some(OasSchemaType::Integer), "Count").unwrap();

        assert_eq!(
            validated.directive,
            ValidatedTypeDirective::Integer(IntegerType::U16)
        );
    }

    #[test]
    fn resolves_auto_integer_type_to_inferred_concrete_directive() {
        let mut schema = schema_with_satay(json!({ "integer-type": "auto" }));
        schema.minimum = Some(1.into());
        schema.maximum = Some(60.into());

        let validated =
            validate_value_satay(&schema, Some(OasSchemaType::Integer), "Count").unwrap();

        assert_eq!(
            validated.directive,
            ValidatedTypeDirective::Integer(IntegerType::U8)
        );
    }

    #[test]
    fn resolves_integer_types_before_unixtime_conversion() {
        for (integer_type, expected) in [("u16", IntegerType::U16), ("auto", IntegerType::I64)] {
            let mut schema = schema_with_satay(json!({ "integer-type": integer_type }));
            schema.format = Some("unixtime".to_owned());

            let validated =
                validate_value_satay(&schema, Some(OasSchemaType::Integer), "Epoch").unwrap();

            assert_eq!(
                validated.directive,
                ValidatedTypeDirective::Integer(expected)
            );
        }
    }

    #[test]
    fn resolves_auto_integer_range_scalar() {
        let mut schema = schema_with_satay(json!({
            "parse-as": "integer-range",
            "integer-type": "auto",
        }));
        schema.minimum = Some(1.into());
        schema.maximum = Some(60.into());

        let validated =
            validate_value_satay(&schema, Some(OasSchemaType::String), "RangeFilter.age").unwrap();

        assert_eq!(
            validated.directive,
            ValidatedTypeDirective::Range(RangeScalar::Integer(IntegerType::U8))
        );
    }

    #[test]
    fn rejects_auto_integer_type_where_absence_is_allowed() {
        for (schema_type, expected_kind) in [
            (Some(OasSchemaType::String), "string"),
            (Some(OasSchemaType::Number), "number"),
            (Some(OasSchemaType::Boolean), "boolean"),
            (None, "missing"),
        ] {
            let auto_schema = schema_with_satay(json!({ "integer-type": "auto" }));
            let error = validation_error(validate_value_satay(&auto_schema, schema_type, "Value"));

            assert!(matches!(
                error,
                ValidationError::SatayIntegerTypeRequiresInteger {
                    context,
                    integer_type,
                    kind,
                } if context == "Value"
                    && integer_type == "auto"
                    && kind == expected_kind
            ));

            let absent_schema = schema_with_satay(json!({}));
            validate_value_satay(&absent_schema, schema_type, "Value")
                .expect("an absent integer type has no placement restriction");
        }
    }

    #[test]
    fn reports_empty_none_if_before_integer_bool_type_conflict() {
        for integer_type in ["i32", "auto"] {
            let schema = schema_with_satay(json!({
                "parse-as": "bool",
                "integer-type": integer_type,
                "none-if": [],
            }));

            let error = validation_error(validate_property_satay(
                &schema,
                Some(OasSchemaType::Integer),
                "User.id",
            ));

            assert!(
                matches!(
                    &error,
                    ValidationError::EmptySatayNoneIf { context }
                        if context == "User.id"
                ),
                "unexpected error for integer type {integer_type}: {error:?}"
            );
        }
    }

    #[test]
    fn rejects_integer_type_for_plain_string_schema() {
        let schema = schema_with_satay(json!({ "integer-type": "i32" }));

        let error = validation_error(validate_value_satay(
            &schema,
            Some(OasSchemaType::String),
            "User.id",
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

        let variants = validate_enum_satay(&schema, &schema.enum_values, "Task.status").unwrap();

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

        let error = validation_error(validate_enum_satay(
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
    fn rejects_enum_variant_overrides_without_enum_values() {
        for mappings in [json!({}), json!({ "active": "Active" })] {
            let schema = schema_with_satay(json!({ "enum-variants": mappings }));

            let error = validation_error(reject_enum_variants_without_enum(&schema, "Task.status"));

            assert!(matches!(
                error,
                ValidationError::SatayEnumVariantsRequireEnum { context }
                    if context == "Task.status"
            ));
        }
    }

    #[test]
    fn rejects_parse_as_with_enum_values() {
        let mut schema = schema_with_satay(json!({ "parse-as": "date" }));
        schema.enum_values = vec![json!("active")];

        let error = validation_error(validate_enum_satay(
            &schema,
            &schema.enum_values,
            "Task.status",
        ));

        assert!(matches!(
            error,
            ValidationError::SatayParseAsWithEnum { context, parse_as }
                if context == "Task.status" && parse_as == "date"
        ));
    }

    #[test]
    fn accepts_treat_error_as_none_only_on_properties() {
        for value in [true, false] {
            let schema = schema_with_satay(json!({ "treat-error-as-none": value }));

            let property =
                validate_property_satay(&schema, Some(OasSchemaType::String), "User.nickname")
                    .unwrap()
                    .property_options
                    .unwrap();
            assert!(
                matches!(property.field_decoding, ValidatedFieldDecoding::Lossy) == value,
                "true enables lossy decoding; false preserves strict decoding"
            );

            let error = validation_error(validate_value_satay(
                &schema,
                Some(OasSchemaType::String),
                "schema `Nickname`",
            ));
            assert!(matches!(
                error,
                ValidationError::SatayTreatErrorAsNoneRequiresObjectProperty { context }
                    if context == "schema `Nickname`"
            ));
        }
    }

    #[test]
    fn accepts_ignore_only_on_object_properties() {
        for value in [true, false] {
            let schema = schema_with_satay(json!({ "ignore": value }));

            let property =
                validate_property_satay(&schema, Some(OasSchemaType::String), "User.metadata")
                    .unwrap()
                    .property_options
                    .unwrap();
            assert_eq!(property.ignore, value);

            let error = validation_error(validate_value_satay(
                &schema,
                Some(OasSchemaType::String),
                "schema `Metadata`",
            ));
            assert!(matches!(
                error,
                ValidationError::SatayIgnoreRequiresObjectProperty { context }
                    if context == "schema `Metadata`"
            ));
        }
    }

    #[test]
    fn validates_identifier_only_on_object_properties() {
        let schema = schema_with_satay(json!({ "identifier": "request-id" }));

        let property =
            validate_property_satay(&schema, Some(OasSchemaType::String), "User.request")
                .unwrap()
                .property_options
                .unwrap();
        assert_eq!(
            property.identifier.as_ref().map(SatayIdentifier::words),
            Some(["request".to_owned(), "id".to_owned()].as_slice())
        );

        let error = validation_error(validate_value_satay(
            &schema,
            Some(OasSchemaType::String),
            "schema `RequestIdentifier`",
        ));
        assert!(matches!(
            error,
            ValidationError::SatayIdentifierRequiresObjectProperty { context }
                if context == "schema `RequestIdentifier`"
        ));
    }

    #[test]
    fn rejects_non_boolean_treat_error_as_none() {
        let schema = schema_with_satay(json!({ "treat-error-as-none": "yes" }));

        let error = validation_error(validate_property_satay(
            &schema,
            Some(OasSchemaType::String),
            "User.nickname",
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
