//! OpenAPI adapter for the shared Rust constraint validator.
use crate::error::ValidationError;
use crate::model::{IntegerType, TypeRef, Validation};
use crate::parse::rust::constraint::{self, ConstraintInput};
pub(in crate::parse) use constraint::reject_keyword;
use oas3::spec::ObjectSchema;

fn input(schema: &ObjectSchema) -> ConstraintInput {
    ConstraintInput {
        format: schema.format.clone(),
        minimum: schema.minimum.clone(),
        maximum: schema.maximum.clone(),
        exclusive_minimum: schema.exclusive_minimum.clone(),
        exclusive_maximum: schema.exclusive_maximum.clone(),
        multiple_of: schema.multiple_of.clone(),
        min_length: schema.min_length,
        max_length: schema.max_length,
        pattern: schema.pattern.clone(),
        min_items: schema.min_items,
        max_items: schema.max_items,
        unique_items: schema.unique_items,
    }
}
pub(super) fn parse_validation(
    schema: &ObjectSchema,
    base: &TypeRef,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    constraint::parse_validation(&input(schema), base, context)
}
pub(crate) fn parse_integer_type(
    schema: &ObjectSchema,
    context: &str,
    explicit: Option<IntegerType>,
) -> Result<IntegerType, ValidationError> {
    constraint::parse_integer_type(&input(schema), context, explicit)
}
