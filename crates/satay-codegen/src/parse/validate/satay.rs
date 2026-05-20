use std::collections::BTreeSet;

use oas3::spec::{ObjectSchema as OasObjectSchema, SchemaType as OasSchemaType};

use super::super::helpers::satay_object;
use super::super::reference::schema_type_wire;
use super::super::satay::{
    parse_range_scalar, parse_satay_enum_variants, parse_satay_integer_type, parse_satay_parse_as,
    satay_parse_as_wire, validate_satay_integer_type,
};
use crate::error::ValidationError;
use crate::model::ParseAs;

pub(super) fn validate_component_enum_satay(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    validate_enum_variants(schema, context)
}

pub(super) fn validate_type_satay(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    allow_treat_error_as_none: bool,
) -> Result<Option<ParseAs>, ValidationError> {
    let parse_as = parse_satay_parse_as(schema, context)?;
    let integer_type = parse_satay_integer_type(schema, context)?;

    validate_satay_integer_type(schema_type, parse_as, integer_type, context)?;

    if let Some(parse_as) = parse_as {
        match (schema_type, parse_as) {
            (Some(OasSchemaType::String), ParseAs::IntegerRange | ParseAs::NumberRange) => {
                parse_range_scalar(schema, parse_as, integer_type, context)?;
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

    if allow_treat_error_as_none {
        validate_treat_error_as_none(schema, context)?;
    }

    Ok(parse_as)
}

pub(super) fn validate_type_enum_satay(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    validate_enum_variants(schema, context)
}

fn validate_enum_variants(schema: &OasObjectSchema, context: &str) -> Result<(), ValidationError> {
    if schema.enum_values.is_empty() {
        return Ok(());
    }

    let mut enum_values = BTreeSet::new();
    for value in &schema.enum_values {
        let Some(value) = value.as_str() else {
            return Ok(());
        };
        enum_values.insert(value.to_owned());
    }

    parse_satay_enum_variants(schema, context, &enum_values)?;
    Ok(())
}

fn validate_treat_error_as_none(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    let Some(satay) = satay_object(schema, context)? else {
        return Ok(());
    };

    let Some(value) = satay.get("treat-error-as-none") else {
        return Ok(());
    };

    value
        .as_bool()
        .ok_or_else(|| ValidationError::InvalidBooleanKeyword {
            context: context.to_owned(),
            keyword: "treat-error-as-none",
        })?;

    Ok(())
}
