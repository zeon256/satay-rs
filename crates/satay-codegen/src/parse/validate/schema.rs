use oas3::spec::{
    ObjectSchema as OasObjectSchema, Schema as OasSchema, SchemaType as OasSchemaType,
};

use super::super::normalize::NormalizedDocument;
use super::super::reference::{
    object_schema, reject_composition, schema_ref, schema_type_and_nullable, schema_type_wire,
};
use super::constraint::{parse_integer_type, parse_validation, reject_keyword};
use super::satay::{
    ValidatedSataySchema, validate_component_enum_satay, validate_type_enum_satay,
    validate_type_satay,
};
use super::state::{ValidatedSchema, ValidatedSchemas};
use crate::error::ValidationError;
use crate::model::{IntegerType, TypeRef};

pub(super) fn validate_components(
    document: &NormalizedDocument<'_>,
    schemas: &mut ValidatedSchemas,
) -> Result<(), ValidationError> {
    let Some(components) = document.resolved.spec.components.as_ref() else {
        return Ok(());
    };

    for (schema_name, schema) in &components.schemas {
        validate_component_schema(document, schema_name, schema, schemas)?;
    }

    Ok(())
}

pub(super) fn validate_type_schema(
    document: &NormalizedDocument<'_>,
    schema: &OasSchema,
    context: &str,
    allow_treat_error_as_none: bool,
    schemas: &mut ValidatedSchemas,
) -> Result<(), ValidationError> {
    if schema_ref(schema, context)?.is_some() {
        return Ok(());
    }

    let schema = object_schema(schema, context)?;
    reject_composition(schema, context)?;
    let schema_id = document.schemas.object_id(schema, context);
    let schema = document.schemas.schema(schema_id, context).schema;

    let (schema_type, _) = schema_type_and_nullable(schema, context)?;
    let mut validated_satay =
        validate_type_satay(schema, schema_type, context, allow_treat_error_as_none)?;

    if validated_satay.parse_as.is_some() {
        schemas.insert_schema(
            schema_id,
            validated_schema_constraints(schema, schema_type, context, validated_satay)?,
        );
        return Ok(());
    }

    if validated_satay.parse_as.is_none() && !schema.enum_values.is_empty() {
        validate_enum_shape(schema, schema_type, context)?;
        validated_satay.enum_variants = validate_type_enum_satay(schema, context)?;
        schemas.insert_schema(
            schema_id,
            validated_schema_constraints(schema, schema_type, context, validated_satay)?,
        );
        return Ok(());
    }

    validate_inline_type_shape(document, schema, schema_type, context, schemas)?;
    schemas.insert_schema(
        schema_id,
        validated_schema_constraints(schema, schema_type, context, validated_satay)?,
    );

    Ok(())
}

fn validate_component_schema(
    document: &NormalizedDocument<'_>,
    schema_name: &str,
    schema: &OasSchema,
    schemas: &mut ValidatedSchemas,
) -> Result<(), ValidationError> {
    let context = format!("schema `{schema_name}`");

    if schema_ref(schema, &context)?.is_some() {
        return Ok(());
    }

    let schema = object_schema(schema, &context)?;
    reject_composition(schema, &context)?;
    let schema_id = document.schemas.object_id(schema, &context);
    let schema = document.schemas.schema(schema_id, &context).schema;

    let (schema_type, _) = schema_type_and_nullable(schema, &context)?;

    if !schema.enum_values.is_empty() {
        validate_enum_shape(schema, schema_type, &context)?;
        let validated_satay = validate_component_enum_satay(schema, &context)?;
        schemas.insert_schema(
            schema_id,
            validated_schema_constraints(schema, schema_type, &context, validated_satay)?,
        );
        return Ok(());
    }

    match schema_type {
        Some(OasSchemaType::Object) | None if !schema.properties.is_empty() => {
            validate_struct_properties(document, schema_name, schema, schemas)?;
        }
        Some(
            OasSchemaType::Array
            | OasSchemaType::String
            | OasSchemaType::Integer
            | OasSchemaType::Number
            | OasSchemaType::Boolean,
        ) => validate_component_alias_satay(document, schema, schema_type, &context, schemas)?,
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
    document: &NormalizedDocument<'_>,
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    schemas: &mut ValidatedSchemas,
) -> Result<(), ValidationError> {
    let schema_id = document.schemas.object_id(schema, context);
    let schema = document.schemas.schema(schema_id, context).schema;
    let validated_satay = validate_type_satay(schema, schema_type, context, false)?;
    let parse_as = validated_satay.parse_as;

    if parse_as.is_some() {
        schemas.insert_schema(
            schema_id,
            validated_schema_constraints(schema, schema_type, context, validated_satay)?,
        );
        return Ok(());
    }

    validate_alias_type_shape(document, schema, schema_type, context, schemas)?;
    schemas.insert_schema(
        schema_id,
        validated_schema_constraints(schema, schema_type, context, validated_satay)?,
    );

    Ok(())
}

fn validate_struct_properties(
    document: &NormalizedDocument<'_>,
    schema_name: &str,
    schema: &OasObjectSchema,
    schemas: &mut ValidatedSchemas,
) -> Result<(), ValidationError> {
    let context = format!("schema `{schema_name}`");
    reject_keyword(schema.min_properties.is_some(), "minProperties", &context)?;
    reject_keyword(schema.max_properties.is_some(), "maxProperties", &context)?;

    for (wire_name, property_schema) in &schema.properties {
        validate_type_schema(
            document,
            property_schema,
            &format!("property `{schema_name}.{wire_name}`"),
            true,
            schemas,
        )?;
    }

    Ok(())
}

fn validate_alias_type_shape(
    document: &NormalizedDocument<'_>,
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    schemas: &mut ValidatedSchemas,
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
            validate_type_schema(document, items, &format!("{context} items"), false, schemas)
        }
        Some(OasSchemaType::String | OasSchemaType::Integer | OasSchemaType::Boolean) => Ok(()),
        _ => Ok(()),
    }
}

fn validate_inline_type_shape(
    document: &NormalizedDocument<'_>,
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    schemas: &mut ValidatedSchemas,
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
            validate_type_schema(document, items, &format!("{context} items"), false, schemas)
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

fn validated_schema_constraints(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    satay: ValidatedSataySchema,
) -> Result<ValidatedSchema, ValidationError> {
    if satay.parse_as.is_some() || !schema.enum_values.is_empty() {
        return Ok(ValidatedSchema {
            satay,
            ..ValidatedSchema::default()
        });
    }

    let integer_type = if schema_type == Some(OasSchemaType::Integer) {
        Some(parse_integer_type(
            schema,
            context,
            satay.explicit_integer_type,
        )?)
    } else {
        None
    };

    let Some(base) = constraint_base_type(schema, schema_type, integer_type, context) else {
        return Ok(ValidatedSchema {
            satay,
            integer_type,
            validation: None,
        });
    };

    Ok(ValidatedSchema {
        validation: parse_validation(schema, &base, context)?,
        satay,
        integer_type,
    })
}

fn constraint_base_type(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    integer_type: Option<IntegerType>,
    context: &str,
) -> Option<TypeRef> {
    match schema_type {
        Some(OasSchemaType::String) => Some(TypeRef::String),
        Some(OasSchemaType::Integer) => Some(TypeRef::Integer(integer_type.unwrap_or_else(|| {
            unreachable!("validation should resolve integer type before constraints for {context}")
        }))),
        Some(OasSchemaType::Number) => Some(validated_number_type(schema, context)),
        Some(OasSchemaType::Boolean) => Some(TypeRef::Bool),
        Some(OasSchemaType::Array) => Some(TypeRef::Array(Box::new(TypeRef::Bool))),
        Some(OasSchemaType::Object) | None => None,
        Some(_) => None,
    }
}

fn validated_number_type(schema: &OasObjectSchema, context: &str) -> TypeRef {
    match schema.format.as_deref() {
        Some("float") => TypeRef::F32,
        Some("double") | None => TypeRef::F64,
        Some(_) => {
            unreachable!("validation should reject unsupported number formats for {context}")
        }
    }
}
