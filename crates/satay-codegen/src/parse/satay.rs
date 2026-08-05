use std::collections::{BTreeMap, BTreeSet};

use oas3::spec::{ObjectSchema as OasObjectSchema, SchemaType as OasSchemaType};

use super::helpers::SataySchemaOptions;
use super::reference::schema_type_wire;
use super::validate::constraint::parse_integer_type;
use crate::error::ValidationError;
use crate::ident::variant_ident;
use crate::model::{IntegerType, ParseAs, RangeScalar};

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
