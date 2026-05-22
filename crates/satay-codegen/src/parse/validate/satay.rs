use std::collections::{BTreeMap, BTreeSet};

use oas3::spec::{ObjectSchema as OasObjectSchema, SchemaType as OasSchemaType};

use super::super::helpers::satay_object;
use super::super::reference::schema_type_wire;
use super::super::satay::{
    parse_range_scalar, parse_satay_enum_variants, parse_satay_integer_type, parse_satay_parse_as,
    satay_parse_as_wire, validate_satay_integer_type,
};
use crate::error::ValidationError;
use crate::model::{IntegerType, ParseAs, RangeScalar};

#[derive(Debug, Clone, Default)]
pub(crate) struct ValidatedSataySchema {
    pub(crate) parse_as: Option<ParseAs>,
    pub(crate) explicit_integer_type: Option<IntegerType>,
    pub(crate) range_scalar: Option<RangeScalar>,
    pub(crate) enum_variants: BTreeMap<String, String>,
    pub(crate) treat_error_as_none: bool,
}

pub(super) fn validate_component_enum_satay(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<ValidatedSataySchema, ValidationError> {
    Ok(ValidatedSataySchema {
        enum_variants: validate_enum_variants(schema, context)?,
        ..ValidatedSataySchema::default()
    })
}

pub(super) fn validate_type_satay(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    allow_treat_error_as_none: bool,
) -> Result<ValidatedSataySchema, ValidationError> {
    let parse_as = parse_satay_parse_as(schema, context)?;
    let explicit_integer_type = parse_satay_integer_type(schema, context)?;
    let mut range_scalar = None;

    validate_satay_integer_type(schema_type, parse_as, explicit_integer_type, context)?;

    if let Some(parse_as) = parse_as {
        match (schema_type, parse_as) {
            (Some(OasSchemaType::String), ParseAs::IntegerRange | ParseAs::NumberRange) => {
                range_scalar = Some(parse_range_scalar(
                    schema,
                    parse_as,
                    explicit_integer_type,
                    context,
                )?);
            }
            (Some(OasSchemaType::String), _) => {}
            (Some(OasSchemaType::Integer), ParseAs::Bool) => {}
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
    }

    Ok(ValidatedSataySchema {
        parse_as,
        explicit_integer_type,
        range_scalar,
        treat_error_as_none: allow_treat_error_as_none
            && validate_treat_error_as_none(schema, context)?,
        ..ValidatedSataySchema::default()
    })
}

pub(super) fn validate_type_enum_satay(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<BTreeMap<String, String>, ValidationError> {
    validate_enum_variants(schema, context)
}

fn validate_enum_variants(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<BTreeMap<String, String>, ValidationError> {
    if schema.enum_values.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut enum_values = BTreeSet::new();
    for value in &schema.enum_values {
        let Some(value) = value.as_str() else {
            return Ok(BTreeMap::new());
        };
        enum_values.insert(value.to_owned());
    }

    parse_satay_enum_variants(schema, context, &enum_values)
}

fn validate_treat_error_as_none(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<bool, ValidationError> {
    let Some(satay) = satay_object(schema, context)? else {
        return Ok(false);
    };

    let Some(value) = satay.get("treat-error-as-none") else {
        return Ok(false);
    };

    let value = value
        .as_bool()
        .ok_or_else(|| ValidationError::InvalidBooleanKeyword {
            context: context.to_owned(),
            keyword: "treat-error-as-none",
        })?;

    Ok(value)
}
