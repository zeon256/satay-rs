use oas3::spec::ObjectSchema as OasObjectSchema;
use serde_json::Number;

use crate::error::ValidationError;
use crate::model::{FloatLimit, IntegerLimit, IntegerType, TypeRef, Validation};

pub(super) fn parse_validation(
    schema: &OasObjectSchema,
    base: &TypeRef,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    match base {
        TypeRef::String => parse_string_validation(schema, context),
        TypeRef::Integer(integer_type) => parse_integer_validation(schema, *integer_type, context),
        TypeRef::F32 | TypeRef::F64 => parse_number_validation(schema, base, context),
        TypeRef::Array(_) => parse_array_validation(schema, context),
        TypeRef::ParsedString(_)
        | TypeRef::ParsedInteger(_)
        | TypeRef::Range(_)
        | TypeRef::Bool
        | TypeRef::Named(_)
        | TypeRef::Constrained { .. }
        | TypeRef::Nullable(_) => Ok(None),
    }
}

fn parse_string_validation(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    let pattern = schema.pattern.clone();

    let min_length = schema.min_length;
    let max_length = schema.max_length;
    if let (Some(min_length), Some(max_length)) = (min_length, max_length)
        && min_length > max_length
    {
        return Err(ValidationError::InvalidStringLengthBounds {
            context: context.to_owned(),
            min_length,
            max_length,
        });
    }

    if pattern.is_some() || min_length.is_some() || max_length.is_some() {
        Ok(Some(Validation::String {
            min_length,
            max_length,
            pattern,
        }))
    } else {
        Ok(None)
    }
}

pub(crate) fn parse_integer_type(
    schema: &OasObjectSchema,
    context: &str,
    explicit: Option<IntegerType>,
) -> Result<IntegerType, ValidationError> {
    let (format_type, explicit_format) = parse_integer_format_type(schema, context)?;
    match explicit {
        Some(integer_type) => Ok(integer_type),
        None => infer_integer_type(schema, format_type, explicit_format, context),
    }
}

fn parse_integer_format_type(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(IntegerType, bool), ValidationError> {
    match schema.format.as_deref() {
        Some("int32") => Ok((IntegerType::I32, true)),
        Some("int64") => Ok((IntegerType::I64, true)),
        None => Ok((IntegerType::I64, false)),
        Some(format) => Err(ValidationError::UnsupportedIntegerFormat {
            context: context.to_owned(),
            format: format.to_owned(),
        }),
    }
}

fn infer_integer_type(
    schema: &OasObjectSchema,
    format_type: IntegerType,
    explicit_format: bool,
    context: &str,
) -> Result<IntegerType, ValidationError> {
    let minimum = optional_integer_minimum(schema, context)?;
    let maximum = optional_integer_maximum(schema, context)?;

    let (minimum, maximum) = match (minimum, maximum) {
        (Some(minimum), Some(maximum)) => (minimum, maximum),
        (Some(minimum), None) => {
            if !explicit_format && effective_integer_min(minimum)? >= 0 {
                return Ok(IntegerType::U64);
            }
            return Ok(format_type);
        }
        _ => return Ok(format_type),
    };

    let raw_minimum = effective_integer_min(minimum)?;
    let raw_maximum = effective_integer_max(maximum)?;

    let (minimum, maximum) =
        intersect_integer_range(raw_minimum, raw_maximum, format_type, context)?;

    if raw_minimum < format_type.min_value() || raw_maximum > format_type.max_value() {
        return Ok(format_type);
    }

    Ok(smallest_integer_type(minimum, maximum))
}

fn smallest_integer_type(minimum: i128, maximum: i128) -> IntegerType {
    if minimum >= 0 {
        if maximum <= i128::from(u8::MAX) {
            IntegerType::U8
        } else if maximum <= i128::from(u16::MAX) {
            IntegerType::U16
        } else if maximum <= i128::from(u32::MAX) {
            IntegerType::U32
        } else {
            IntegerType::U64
        }
    } else if minimum >= i128::from(i8::MIN) && maximum <= i128::from(i8::MAX) {
        IntegerType::I8
    } else if minimum >= i128::from(i16::MIN) && maximum <= i128::from(i16::MAX) {
        IntegerType::I16
    } else if minimum >= i128::from(i32::MIN) && maximum <= i128::from(i32::MAX) {
        IntegerType::I32
    } else {
        IntegerType::I64
    }
}

fn parse_integer_validation(
    schema: &OasObjectSchema,
    integer_type: IntegerType,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    reject_keyword(schema.multiple_of.is_some(), "multipleOf", context)?;

    let minimum = optional_integer_minimum(schema, context)?;
    let maximum = optional_integer_maximum(schema, context)?;

    let (minimum, maximum) = normalize_integer_limits(minimum, maximum, integer_type, context)?;

    if minimum.is_some() || maximum.is_some() {
        Ok(Some(Validation::Integer { minimum, maximum }))
    } else {
        Ok(None)
    }
}

fn parse_number_validation(
    schema: &OasObjectSchema,
    base: &TypeRef,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    reject_keyword(schema.multiple_of.is_some(), "multipleOf", context)?;

    let minimum = optional_float_minimum(schema, context)?;
    let maximum = optional_float_maximum(schema, context)?;
    let (minimum, maximum) = normalize_float_limits(minimum, maximum, base, context)?;

    if minimum.is_some() || maximum.is_some() {
        Ok(Some(Validation::Number { minimum, maximum }))
    } else {
        Ok(None)
    }
}

fn parse_array_validation(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    if schema.unique_items.unwrap_or(false) {
        return Err(ValidationError::UniqueItemsUnsupported {
            context: context.to_owned(),
        });
    }

    let min_items = schema.min_items;
    let max_items = schema.max_items;

    if let (Some(min_items), Some(max_items)) = (min_items, max_items)
        && min_items > max_items
    {
        return Err(ValidationError::InvalidArrayLengthBounds {
            context: context.to_owned(),
            min_items,
            max_items,
        });
    }

    if min_items.is_some() || max_items.is_some() {
        Ok(Some(Validation::Array {
            min_items,
            max_items,
        }))
    } else {
        Ok(None)
    }
}

pub(super) fn reject_keyword(
    present: bool,
    keyword: &'static str,
    context: &str,
) -> Result<(), ValidationError> {
    if present {
        return Err(ValidationError::UnsupportedKeyword {
            context: context.to_owned(),
            keyword,
        });
    }
    Ok(())
}

fn optional_integer_minimum(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<Option<IntegerLimit>, ValidationError> {
    tighter_integer_minimum(
        schema
            .minimum
            .as_ref()
            .map(|value| integer_limit(value, false, "minimum", context))
            .transpose()?,
        schema
            .exclusive_minimum
            .as_ref()
            .map(|value| integer_limit(value, true, "exclusiveMinimum", context))
            .transpose()?,
    )
}

fn optional_integer_maximum(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<Option<IntegerLimit>, ValidationError> {
    tighter_integer_maximum(
        schema
            .maximum
            .as_ref()
            .map(|value| integer_limit(value, false, "maximum", context))
            .transpose()?,
        schema
            .exclusive_maximum
            .as_ref()
            .map(|value| integer_limit(value, true, "exclusiveMaximum", context))
            .transpose()?,
    )
}

fn integer_limit(
    value: &Number,
    exclusive: bool,
    keyword: &'static str,
    context: &str,
) -> Result<IntegerLimit, ValidationError> {
    Ok(IntegerLimit {
        value: json_integer(value, &format!("{context}.{keyword}"))?,
        exclusive,
    })
}

fn tighter_integer_minimum(
    inclusive: Option<IntegerLimit>,
    exclusive: Option<IntegerLimit>,
) -> Result<Option<IntegerLimit>, ValidationError> {
    match (inclusive, exclusive) {
        (Some(inclusive), Some(exclusive)) => {
            let inclusive_effective = effective_integer_min(inclusive)?;
            let exclusive_effective = effective_integer_min(exclusive)?;
            if exclusive_effective > inclusive_effective {
                Ok(Some(exclusive))
            } else {
                Ok(Some(inclusive))
            }
        }
        (Some(limit), None) | (None, Some(limit)) => Ok(Some(limit)),
        (None, None) => Ok(None),
    }
}

fn tighter_integer_maximum(
    inclusive: Option<IntegerLimit>,
    exclusive: Option<IntegerLimit>,
) -> Result<Option<IntegerLimit>, ValidationError> {
    match (inclusive, exclusive) {
        (Some(inclusive), Some(exclusive)) => {
            let inclusive_effective = effective_integer_max(inclusive)?;
            let exclusive_effective = effective_integer_max(exclusive)?;
            if exclusive_effective < inclusive_effective {
                Ok(Some(exclusive))
            } else {
                Ok(Some(inclusive))
            }
        }
        (Some(limit), None) | (None, Some(limit)) => Ok(Some(limit)),
        (None, None) => Ok(None),
    }
}

fn optional_float_minimum(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<Option<FloatLimit>, ValidationError> {
    Ok(tighter_float_minimum(
        schema
            .minimum
            .as_ref()
            .map(|value| float_limit(value, false, "minimum", context))
            .transpose()?,
        schema
            .exclusive_minimum
            .as_ref()
            .map(|value| float_limit(value, true, "exclusiveMinimum", context))
            .transpose()?,
    ))
}

fn optional_float_maximum(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<Option<FloatLimit>, ValidationError> {
    Ok(tighter_float_maximum(
        schema
            .maximum
            .as_ref()
            .map(|value| float_limit(value, false, "maximum", context))
            .transpose()?,
        schema
            .exclusive_maximum
            .as_ref()
            .map(|value| float_limit(value, true, "exclusiveMaximum", context))
            .transpose()?,
    ))
}

fn float_limit(
    value: &Number,
    exclusive: bool,
    keyword: &'static str,
    context: &str,
) -> Result<FloatLimit, ValidationError> {
    let value = value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| ValidationError::InvalidFiniteNumberKeyword {
            context: context.to_owned(),
            keyword,
        })?;
    Ok(FloatLimit { value, exclusive })
}

fn tighter_float_minimum(
    inclusive: Option<FloatLimit>,
    exclusive: Option<FloatLimit>,
) -> Option<FloatLimit> {
    match (inclusive, exclusive) {
        (Some(inclusive), Some(exclusive)) => {
            if exclusive.value > inclusive.value
                || (exclusive.value == inclusive.value && !inclusive.exclusive)
            {
                Some(exclusive)
            } else {
                Some(inclusive)
            }
        }
        (Some(limit), None) | (None, Some(limit)) => Some(limit),
        (None, None) => None,
    }
}

fn tighter_float_maximum(
    inclusive: Option<FloatLimit>,
    exclusive: Option<FloatLimit>,
) -> Option<FloatLimit> {
    match (inclusive, exclusive) {
        (Some(inclusive), Some(exclusive)) => {
            if exclusive.value < inclusive.value
                || (exclusive.value == inclusive.value && !inclusive.exclusive)
            {
                Some(exclusive)
            } else {
                Some(inclusive)
            }
        }
        (Some(limit), None) | (None, Some(limit)) => Some(limit),
        (None, None) => None,
    }
}

fn json_integer(value: &Number, context: &str) -> Result<i128, ValidationError> {
    if let Some(value) = value.as_i64() {
        return Ok(i128::from(value));
    }
    if let Some(value) = value.as_u64() {
        return Ok(i128::from(value));
    }
    let Some(value) = value.as_f64() else {
        return Err(ValidationError::ExpectedInteger {
            context: context.to_owned(),
        });
    };
    if !value.is_finite() || value.fract() != 0.0 {
        return Err(ValidationError::ExpectedInteger {
            context: context.to_owned(),
        });
    }
    Ok(value as i128)
}

fn normalize_integer_limits(
    minimum: Option<IntegerLimit>,
    maximum: Option<IntegerLimit>,
    integer_type: IntegerType,
    context: &str,
) -> Result<(Option<IntegerLimit>, Option<IntegerLimit>), ValidationError> {
    let type_min = integer_type.min_value();
    let type_max = integer_type.max_value();

    let raw_minimum = minimum
        .map(effective_integer_min)
        .transpose()?
        .unwrap_or(type_min);

    let raw_maximum = maximum
        .map(effective_integer_max)
        .transpose()?
        .unwrap_or(type_max);

    intersect_integer_range(raw_minimum, raw_maximum, integer_type, context)?;

    let minimum = minimum.filter(|_| raw_minimum > type_min);
    let maximum = maximum.filter(|_| raw_maximum < type_max);

    Ok((minimum, maximum))
}

fn intersect_integer_range(
    minimum: i128,
    maximum: i128,
    integer_type: IntegerType,
    context: &str,
) -> Result<(i128, i128), ValidationError> {
    let minimum = minimum.max(integer_type.min_value());
    let maximum = maximum.min(integer_type.max_value());

    if minimum > maximum {
        return Err(ValidationError::EmptyIntegerBounds {
            context: context.to_owned(),
        });
    }

    Ok((minimum, maximum))
}

fn effective_integer_min(limit: IntegerLimit) -> Result<i128, ValidationError> {
    if limit.exclusive {
        limit
            .value
            .checked_add(1)
            .ok_or(ValidationError::ExclusiveIntegerMinimumOverflow)
    } else {
        Ok(limit.value)
    }
}

fn effective_integer_max(limit: IntegerLimit) -> Result<i128, ValidationError> {
    if limit.exclusive {
        limit
            .value
            .checked_sub(1)
            .ok_or(ValidationError::ExclusiveIntegerMaximumOverflow)
    } else {
        Ok(limit.value)
    }
}

fn normalize_float_limits(
    minimum: Option<FloatLimit>,
    maximum: Option<FloatLimit>,
    base: &TypeRef,
    context: &str,
) -> Result<(Option<FloatLimit>, Option<FloatLimit>), ValidationError> {
    let (type_min, type_max) = match base {
        TypeRef::F32 => (f64::from(f32::MIN), f64::from(f32::MAX)),
        TypeRef::F64 => (f64::MIN, f64::MAX),
        _ => unreachable!("number validation is only parsed for number types"),
    };

    let effective_min = minimum.map(|limit| limit.value).unwrap_or(type_min);
    let effective_max = maximum.map(|limit| limit.value).unwrap_or(type_max);

    if effective_min > effective_max
        || (effective_min == effective_max
            && minimum.is_some_and(|limit| limit.exclusive)
            && maximum.is_some_and(|limit| limit.exclusive))
    {
        return Err(ValidationError::EmptyNumberBounds {
            context: context.to_owned(),
        });
    }

    let minimum = minimum.filter(|limit| limit.value > type_min);
    let maximum = maximum.filter(|limit| limit.value < type_max);

    Ok((minimum, maximum))
}
