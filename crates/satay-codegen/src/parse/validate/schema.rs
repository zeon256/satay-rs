use oas3::spec::{
    ObjectSchema as OasObjectSchema, Schema as OasSchema, SchemaType as OasSchemaType,
};

use super::super::reference::{
    object_schema, reject_composition, schema_ref, schema_type_and_nullable,
};
use super::super::resolve::ResolvedDocument;
use super::satay::{validate_component_enum_satay, validate_type_enum_satay, validate_type_satay};
use crate::error::ValidationError;

pub(super) fn validate_components(document: &ResolvedDocument<'_>) -> Result<(), ValidationError> {
    let Some(components) = document.spec.components.as_ref() else {
        return Ok(());
    };

    for (schema_name, schema) in &components.schemas {
        validate_component_schema(schema_name, schema)?;
    }

    Ok(())
}

pub(super) fn validate_type_schema(
    schema: &OasSchema,
    context: &str,
    allow_treat_error_as_none: bool,
) -> Result<(), ValidationError> {
    if schema_ref(schema, context)?.is_some() {
        return Ok(());
    }

    let schema = object_schema(schema, context)?;

    if has_composition(schema) {
        return Ok(());
    }

    reject_composition(schema, context)?;

    let (schema_type, _) = schema_type_and_nullable(schema, context)?;
    let parse_as = validate_type_satay(schema, schema_type, context, allow_treat_error_as_none)?;

    if parse_as.is_none() && !schema.enum_values.is_empty() {
        validate_type_enum_satay(schema, context)?;
        return Ok(());
    }

    if parse_as.is_none() && schema_type == Some(OasSchemaType::Array)
        && let Some(items) = schema.items.as_deref() {
            validate_type_schema(items, &format!("{context} items"), false)?;
        }

    Ok(())
}

fn validate_component_schema(schema_name: &str, schema: &OasSchema) -> Result<(), ValidationError> {
    let context = format!("schema `{schema_name}`");

    if schema_ref(schema, &context)?.is_some() {
        return Ok(());
    }

    let schema = object_schema(schema, &context)?;

    if has_composition(schema) {
        return Ok(());
    }

    reject_composition(schema, &context)?;

    let (schema_type, _) = schema_type_and_nullable(schema, &context)?;

    if !schema.enum_values.is_empty() {
        validate_component_enum_satay(schema, &context)?;
        return Ok(());
    }

    match schema_type {
        Some(OasSchemaType::Object) | None if !schema.properties.is_empty() => {
            validate_struct_properties(schema_name, schema)?;
        }
        Some(
            OasSchemaType::Array
            | OasSchemaType::String
            | OasSchemaType::Integer
            | OasSchemaType::Number
            | OasSchemaType::Boolean,
        ) => validate_component_alias_satay(schema, schema_type, &context)?,
        Some(_) | None => {}
    }

    Ok(())
}

fn validate_component_alias_satay(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
) -> Result<(), ValidationError> {
    let parse_as = validate_type_satay(schema, schema_type, context, false)?;

    if parse_as.is_none() && schema_type == Some(OasSchemaType::Array)
        && let Some(items) = schema.items.as_deref() {
            validate_type_schema(items, &format!("{context} items"), false)?;
        }

    Ok(())
}

fn validate_struct_properties(
    schema_name: &str,
    schema: &OasObjectSchema,
) -> Result<(), ValidationError> {
    for (wire_name, property_schema) in &schema.properties {
        validate_type_schema(
            property_schema,
            &format!("property `{schema_name}.{wire_name}`"),
            true,
        )?;
    }

    Ok(())
}

fn has_composition(schema: &OasObjectSchema) -> bool {
    !schema.one_of.is_empty() || !schema.any_of.is_empty() || !schema.all_of.is_empty()
}
