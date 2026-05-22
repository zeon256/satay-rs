use oas3::spec::{
    ObjectSchema as OasObjectSchema, Schema as OasSchema, SchemaType as OasSchemaType,
};

use super::super::reference::{
    object_schema, reject_composition, schema_ref, schema_type_and_nullable, schema_type_wire,
};
use super::super::resolve::ResolvedDocument;
use super::satay::{
    ValidatedSatay, validate_component_enum_satay, validate_type_enum_satay, validate_type_satay,
};
use crate::error::ValidationError;

pub(super) fn validate_components(
    document: &ResolvedDocument<'_>,
    satay: &mut ValidatedSatay,
) -> Result<(), ValidationError> {
    let Some(components) = document.spec.components.as_ref() else {
        return Ok(());
    };

    for (schema_name, schema) in &components.schemas {
        validate_component_schema(schema_name, schema, satay)?;
    }

    Ok(())
}

pub(super) fn validate_type_schema(
    schema: &OasSchema,
    context: &str,
    allow_treat_error_as_none: bool,
    satay: &mut ValidatedSatay,
) -> Result<(), ValidationError> {
    if schema_ref(schema, context)?.is_some() {
        return Ok(());
    }

    let schema = object_schema(schema, context)?;
    reject_composition(schema, context)?;

    let (schema_type, _) = schema_type_and_nullable(schema, context)?;
    let mut validated_satay =
        validate_type_satay(schema, schema_type, context, allow_treat_error_as_none)?;

    if validated_satay.parse_as.is_some() {
        satay.insert_schema(schema, validated_satay);
        return Ok(());
    }

    if validated_satay.parse_as.is_none() && !schema.enum_values.is_empty() {
        validate_enum_shape(schema, schema_type, context)?;
        validated_satay.enum_variants = validate_type_enum_satay(schema, context)?;
        satay.insert_schema(schema, validated_satay);
        return Ok(());
    }

    satay.insert_schema(schema, validated_satay);
    validate_inline_type_shape(schema, schema_type, context, satay)?;

    Ok(())
}

fn validate_component_schema(
    schema_name: &str,
    schema: &OasSchema,
    satay: &mut ValidatedSatay,
) -> Result<(), ValidationError> {
    let context = format!("schema `{schema_name}`");

    if schema_ref(schema, &context)?.is_some() {
        return Ok(());
    }

    let schema = object_schema(schema, &context)?;
    reject_composition(schema, &context)?;

    let (schema_type, _) = schema_type_and_nullable(schema, &context)?;

    if !schema.enum_values.is_empty() {
        validate_enum_shape(schema, schema_type, &context)?;
        let validated_satay = validate_component_enum_satay(schema, &context)?;
        satay.insert_schema(schema, validated_satay);
        return Ok(());
    }

    match schema_type {
        Some(OasSchemaType::Object) | None if !schema.properties.is_empty() => {
            validate_struct_properties(schema_name, schema, satay)?;
        }
        Some(
            OasSchemaType::Array
            | OasSchemaType::String
            | OasSchemaType::Integer
            | OasSchemaType::Number
            | OasSchemaType::Boolean,
        ) => validate_component_alias_satay(schema, schema_type, &context, satay)?,
        Some(kind) => {
            return Err(ValidationError::UnsupportedComponentType {
                schema: schema_name.to_owned(),
                kind: schema_type_wire(kind).to_owned(),
            });
        }
        None => {
            return Err(ValidationError::MissingComponentSchemaType {
                schema: schema_name.to_owned(),
            });
        }
    }

    Ok(())
}

fn validate_component_alias_satay(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    satay: &mut ValidatedSatay,
) -> Result<(), ValidationError> {
    let validated_satay = validate_type_satay(schema, schema_type, context, false)?;
    let parse_as = validated_satay.parse_as;
    satay.insert_schema(schema, validated_satay);

    if parse_as.is_some() {
        return Ok(());
    }

    validate_alias_type_shape(schema, schema_type, context, satay)?;

    Ok(())
}

fn validate_struct_properties(
    schema_name: &str,
    schema: &OasObjectSchema,
    satay: &mut ValidatedSatay,
) -> Result<(), ValidationError> {
    for (wire_name, property_schema) in &schema.properties {
        validate_type_schema(
            property_schema,
            &format!("property `{schema_name}.{wire_name}`"),
            true,
            satay,
        )?;
    }

    Ok(())
}

fn validate_alias_type_shape(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    satay: &mut ValidatedSatay,
) -> Result<(), ValidationError> {
    match schema_type {
        Some(OasSchemaType::Number) => validate_number_format(schema, context),
        Some(OasSchemaType::Array) => {
            let items =
                schema
                    .items
                    .as_deref()
                    .ok_or_else(|| ValidationError::MissingArrayItems {
                        context: context.to_owned(),
                    })?;
            validate_type_schema(items, &format!("{context} items"), false, satay)
        }
        Some(OasSchemaType::String | OasSchemaType::Integer | OasSchemaType::Boolean) => Ok(()),
        _ => Ok(()),
    }
}

fn validate_inline_type_shape(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    satay: &mut ValidatedSatay,
) -> Result<(), ValidationError> {
    match schema_type {
        Some(OasSchemaType::String | OasSchemaType::Integer | OasSchemaType::Boolean) => Ok(()),
        Some(OasSchemaType::Number) => validate_number_format(schema, context),
        Some(OasSchemaType::Array) => {
            let items =
                schema
                    .items
                    .as_deref()
                    .ok_or_else(|| ValidationError::MissingArrayItems {
                        context: context.to_owned(),
                    })?;
            validate_type_schema(items, &format!("{context} items"), false, satay)
        }
        Some(OasSchemaType::Object) | None if !schema.properties.is_empty() => {
            Err(ValidationError::InlineObjectSchema {
                context: context.to_owned(),
            })
        }
        Some(OasSchemaType::Object) => Err(ValidationError::UnsupportedMapObjectSchema {
            context: context.to_owned(),
        }),
        Some(kind) => Err(ValidationError::UnsupportedSchemaType {
            context: context.to_owned(),
            kind: schema_type_wire(kind).to_owned(),
        }),
        None => Err(ValidationError::MissingSchemaType {
            context: context.to_owned(),
        }),
    }
}

fn validate_enum_shape(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
) -> Result<(), ValidationError> {
    if let Some(kind) = schema_type
        && kind != OasSchemaType::String
    {
        return Err(ValidationError::UnsupportedEnumType {
            context: context.to_owned(),
            kind: schema_type_wire(kind).to_owned(),
        });
    }

    if schema.enum_values.is_empty() {
        return Err(ValidationError::EmptyEnum {
            context: context.to_owned(),
        });
    }

    for value in &schema.enum_values {
        if value.as_str().is_none() {
            return Err(ValidationError::NonStringEnumValue {
                context: context.to_owned(),
            });
        }
    }

    Ok(())
}

fn validate_number_format(schema: &OasObjectSchema, context: &str) -> Result<(), ValidationError> {
    match schema.format.as_deref() {
        Some("float" | "double") | None => Ok(()),
        Some(format) => Err(ValidationError::UnsupportedNumberFormat {
            context: context.to_owned(),
            format: format.to_owned(),
        }),
    }
}
