//! Numeric and length constraint normalization for the IR frontend.

use std::cmp::Ordering;

use oas3::spec::ObjectSchema as OasObjectSchema;
use serde_json::Number;

use crate::error::ValidationError;
use crate::parse::rust::constraint::{json_integer, reject_keyword};

use satay_ir::{ArrayConstraints, NumericBound, NumericConstraints, StringConstraints};

/// Normalizes declared numeric bounds for an integer (`true`) or number
/// (`false`) schema.
///
/// For each side the tighter inclusive/exclusive declaration wins; at equal
/// values the exclusive declaration wins. The original declared [`Number`] is
/// retained. Integer comparisons reuse the raw `json_integer` scalar domain;
/// floating comparisons use finite `as_f64` values, matching the existing
/// parser's numeric domain. Declared intervals that contain no value are
/// rejected, including integer intervals made empty by exclusive endpoints.
///
/// # Errors
///
/// Returns a [`ValidationError`] for an unsupported or empty declaration.
pub(in crate::parse) fn numeric_constraints(
    schema: &OasObjectSchema,
    integer: bool,
    context: &str,
    recover: bool,
) -> Result<NumericConstraints, ValidationError> {
    if recover {
        let mut constraints =
            numeric_constraints(schema, integer, context, false).unwrap_or_default();
        constraints.declared = Some(satay_ir::DeclaredNumericConstraints {
            minimum: schema.minimum.clone(),
            exclusive_minimum: schema.exclusive_minimum.clone(),
            maximum: schema.maximum.clone(),
            exclusive_maximum: schema.exclusive_maximum.clone(),
            multiple_of: schema.multiple_of.clone(),
        });
        return Ok(constraints);
    }
    reject_keyword(schema.multiple_of.is_some(), "multipleOf", context)?;

    let minimum = tighter_bound(
        schema.minimum.as_ref(),
        schema.exclusive_minimum.as_ref(),
        true,
        integer,
        context,
    )?;
    let maximum = tighter_bound(
        schema.maximum.as_ref(),
        schema.exclusive_maximum.as_ref(),
        false,
        integer,
        context,
    )?;

    check_interval(&minimum, &maximum, integer, context)?;

    Ok(NumericConstraints {
        declared: None,
        minimum,
        maximum,
    })
}

/// Normalizes declared string length and pattern constraints.
///
/// # Errors
///
/// Returns a [`ValidationError`] when `minLength` exceeds `maxLength`.
pub(in crate::parse) fn string_constraints(
    schema: &OasObjectSchema,
    context: &str,
    recover: bool,
) -> Result<StringConstraints, ValidationError> {
    if let (Some(min_length), Some(max_length)) = (schema.min_length, schema.max_length)
        && min_length > max_length
        && !recover
    {
        return Err(ValidationError::InvalidStringLengthBounds {
            context: context.to_owned(),
            min_length,
            max_length,
        });
    }

    Ok(StringConstraints {
        min_length: schema.min_length,
        max_length: schema.max_length,
        pattern: schema.pattern.clone(),
    })
}

/// Normalizes declared array length constraints and rejects `uniqueItems`.
///
/// # Errors
///
/// Returns a [`ValidationError`] for `uniqueItems: true` or inverted bounds.
pub(in crate::parse) fn array_constraints(
    schema: &OasObjectSchema,
    context: &str,
    recover: bool,
) -> Result<ArrayConstraints, ValidationError> {
    if !recover && schema.unique_items == Some(true) {
        return Err(ValidationError::UniqueItemsUnsupported {
            context: context.to_owned(),
        });
    }

    if let (Some(min_items), Some(max_items)) = (schema.min_items, schema.max_items)
        && min_items > max_items
        && !recover
    {
        return Err(ValidationError::InvalidArrayLengthBounds {
            context: context.to_owned(),
            min_items,
            max_items,
        });
    }

    Ok(ArrayConstraints {
        unique_items: schema.unique_items.unwrap_or(false),
        min_items: schema.min_items,
        max_items: schema.max_items,
    })
}

/// Selects the tighter of the inclusive and exclusive declarations for one
/// side, keeping the original declared [`Number`].
fn tighter_bound(
    inclusive: Option<&Number>,
    exclusive: Option<&Number>,
    lower: bool,
    integer: bool,
    context: &str,
) -> Result<Option<NumericBound>, ValidationError> {
    let inclusive = inclusive
        .map(|value| declared_bound(value, false, lower, integer, context))
        .transpose()?;
    let exclusive = exclusive
        .map(|value| declared_bound(value, true, lower, integer, context))
        .transpose()?;

    match (inclusive, exclusive) {
        (Some(inclusive), Some(exclusive)) => {
            let exclusive_wins = if lower {
                compare(&exclusive.value, &inclusive.value, integer, context)? != Ordering::Less
            } else {
                compare(&exclusive.value, &inclusive.value, integer, context)? != Ordering::Greater
            };
            Ok(Some(if exclusive_wins { exclusive } else { inclusive }))
        }
        (bound @ Some(_), None) | (None, bound @ Some(_)) => Ok(bound),
        (None, None) => Ok(None),
    }
}

/// Validates one declared bound and retains its parsed value.
fn declared_bound(
    value: &Number,
    exclusive: bool,
    lower: bool,
    integer: bool,
    context: &str,
) -> Result<NumericBound, ValidationError> {
    let keyword = match (lower, exclusive) {
        (true, false) => "minimum",
        (true, true) => "exclusiveMinimum",
        (false, false) => "maximum",
        (false, true) => "exclusiveMaximum",
    };
    let location = format!("{context}.{keyword}");

    if integer {
        json_integer(value, &location)?;
    } else {
        let _ = finite_value(value, &location, keyword)?;
    }

    Ok(NumericBound {
        value: value.clone(),
        exclusive,
    })
}

/// Compares two bound values in the schema's numeric domain.
fn compare(
    left: &Number,
    right: &Number,
    integer: bool,
    context: &str,
) -> Result<Ordering, ValidationError> {
    if integer {
        let left = json_integer(left, context)?;
        let right = json_integer(right, context)?;
        return Ok(left.cmp(&right));
    }

    let left = finite_value(left, context, "minimum")?;
    let right = finite_value(right, context, "maximum")?;
    left.partial_cmp(&right)
        .ok_or_else(|| ValidationError::InvalidFiniteNumberKeyword {
            context: context.to_owned(),
            keyword: "minimum",
        })
}

fn finite_value(
    value: &Number,
    context: &str,
    keyword: &'static str,
) -> Result<f64, ValidationError> {
    value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| ValidationError::InvalidFiniteNumberKeyword {
            context: context.to_owned(),
            keyword,
        })
}

/// Rejects declared intervals that contain no value.
fn check_interval(
    minimum: &Option<NumericBound>,
    maximum: &Option<NumericBound>,
    integer: bool,
    context: &str,
) -> Result<(), ValidationError> {
    let (Some(minimum), Some(maximum)) = (minimum, maximum) else {
        return Ok(());
    };

    if integer {
        let minimum_value = json_integer(&minimum.value, context)?;
        let maximum_value = json_integer(&maximum.value, context)?;
        let effective_minimum = if minimum.exclusive {
            minimum_value.checked_add(1)
        } else {
            Some(minimum_value)
        };
        let effective_maximum = if maximum.exclusive {
            maximum_value.checked_sub(1)
        } else {
            Some(maximum_value)
        };
        let empty = match (effective_minimum, effective_maximum) {
            (Some(minimum), Some(maximum)) => minimum > maximum,
            // An endpoint at the `i128` boundary cannot be violated from the
            // opposite side within this numeric domain.
            _ => false,
        };
        if empty {
            return Err(ValidationError::EmptyIntegerBounds {
                context: context.to_owned(),
            });
        }
        return Ok(());
    }

    let minimum_value = finite_value(&minimum.value, context, "minimum")?;
    let maximum_value = finite_value(&maximum.value, context, "maximum")?;
    if minimum_value > maximum_value
        || (minimum_value == maximum_value && (minimum.exclusive || maximum.exclusive))
    {
        return Err(ValidationError::EmptyNumberBounds {
            context: context.to_owned(),
        });
    }
    Ok(())
}
