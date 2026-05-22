use std::collections::{BTreeMap, BTreeSet};

use oas3::spec::{
    ObjectSchema as OasObjectSchema, Schema as OasSchema, SchemaType as OasSchemaType,
};

use super::super::helpers::{optional_description, schema_description};
use super::super::reference::{
    object_schema, reject_composition, schema_ref, schema_ref_type_name, schema_type_and_nullable,
    schema_type_wire,
};
use super::super::resolve::ResolvedDocument;
use super::constraint::{parse_integer_type, parse_validation, reject_keyword};
use super::satay::{
    ValidatedParseAs, ValidatedSataySchema, validate_component_enum_satay,
    validate_type_enum_satay, validate_type_satay,
};
use super::{
    ValidatedComponent, ValidatedComponentKind, ValidatedField, ValidatedType, ValidatedTypeKind,
};
use crate::error::ValidationError;
use crate::ident::{unique_ident, variant_ident};
use crate::model::{EnumVariant, TypeRef};

pub(super) fn validate_components(
    document: &ResolvedDocument<'_>,
) -> Result<Vec<ValidatedComponent>, ValidationError> {
    let Some(components) = document.spec.components.as_ref() else {
        return Ok(vec![]);
    };

    let mut parsed = Vec::with_capacity(components.schemas.len());

    for (schema_name, schema) in &components.schemas {
        parsed.push(validate_component_schema(document, schema_name, schema)?);
    }

    Ok(parsed)
}

pub(super) fn validate_type_schema(
    document: &ResolvedDocument<'_>,
    schema: &OasSchema,
    context: &str,
    allow_treat_error_as_none: bool,
) -> Result<ValidatedType, ValidationError> {
    if let Some(reference) = schema_ref(schema, context)? {
        return Ok(ValidatedType::named(schema_ref_type_name(reference)?));
    }

    let schema = object_schema(schema, context)?;
    reject_composition(schema, context)?;
    let (schema_type, nullable) = schema_type_and_nullable(schema, context)?;

    validate_object_type_schema(
        document,
        schema,
        schema_type,
        nullable,
        context,
        allow_treat_error_as_none,
    )
}

fn validate_component_schema(
    document: &ResolvedDocument<'_>,
    schema_name: &str,
    schema: &OasSchema,
) -> Result<ValidatedComponent, ValidationError> {
    let context = format!("schema `{schema_name}`");
    let description = schema_description(schema);
    let kind = if let Some(reference) = schema_ref(schema, &context)? {
        ValidatedComponentKind::Reference(schema_ref_type_name(reference)?)
    } else {
        let schema = object_schema(schema, &context)?;
        reject_composition(schema, &context)?;
        let (schema_type, nullable) = schema_type_and_nullable(schema, &context)?;

        if !schema.enum_values.is_empty() {
            validate_enum_shape(schema, schema_type, &context)?;
            let validated_satay = validate_component_enum_satay(schema, &context)?;
            ValidatedComponentKind::Type(ValidatedType {
                kind: ValidatedTypeKind::Enum(validated_enum_variants(
                    schema,
                    &validated_satay.enum_variants,
                    &context,
                )?),
                nullable,
                validation: None,
                description: optional_description(&schema.description),
                treat_error_as_none: false,
            })
        } else {
            match schema_type {
                Some(OasSchemaType::Object) | None if !schema.properties.is_empty() => {
                    ValidatedComponentKind::Struct(validate_struct_properties(
                        document,
                        schema_name,
                        schema,
                    )?)
                }
                Some(
                    OasSchemaType::Array
                    | OasSchemaType::String
                    | OasSchemaType::Integer
                    | OasSchemaType::Number
                    | OasSchemaType::Boolean,
                ) => ValidatedComponentKind::Type(validate_object_type_schema(
                    document,
                    schema,
                    schema_type,
                    nullable,
                    &context,
                    false,
                )?),
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
        }
    };

    Ok(ValidatedComponent {
        schema_name: schema_name.to_owned(),
        description,
        kind,
    })
}

fn validate_object_type_schema(
    document: &ResolvedDocument<'_>,
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    nullable: bool,
    context: &str,
    allow_treat_error_as_none: bool,
) -> Result<ValidatedType, ValidationError> {
    let description = optional_description(&schema.description);
    let validated_satay =
        validate_type_satay(schema, schema_type, context, allow_treat_error_as_none)?;

    if let Some(parse_as) = validated_satay.parse_as {
        return Ok(ValidatedType {
            kind: validated_parse_as_kind(parse_as),
            nullable,
            validation: None,
            description,
            treat_error_as_none: validated_satay.treat_error_as_none,
        });
    }

    if !schema.enum_values.is_empty() {
        validate_enum_shape(schema, schema_type, context)?;
        let explicit_variants = validate_type_enum_satay(schema, context)?;
        return Ok(ValidatedType {
            kind: ValidatedTypeKind::Enum(validated_enum_variants(
                schema,
                &explicit_variants,
                context,
            )?),
            nullable,
            validation: None,
            description,
            treat_error_as_none: validated_satay.treat_error_as_none,
        });
    }

    let kind = validate_inline_type_kind(document, schema, schema_type, context, &validated_satay)?;
    let validation = validation_base_type(&kind)
        .map(|base| parse_validation(schema, &base, context))
        .transpose()?
        .flatten();

    Ok(ValidatedType {
        kind,
        nullable,
        validation,
        description,
        treat_error_as_none: validated_satay.treat_error_as_none,
    })
}

fn validated_parse_as_kind(parse_as: ValidatedParseAs) -> ValidatedTypeKind {
    match parse_as {
        ValidatedParseAs::ParsedString(parse_as) => ValidatedTypeKind::ParsedString(parse_as),
        ValidatedParseAs::ParsedInteger(parse_as) => ValidatedTypeKind::ParsedInteger(parse_as),
        ValidatedParseAs::Range(scalar) => ValidatedTypeKind::Range(scalar),
    }
}

fn validate_inline_type_kind(
    document: &ResolvedDocument<'_>,
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    satay: &ValidatedSataySchema,
) -> Result<ValidatedTypeKind, ValidationError> {
    match schema_type {
        Some(OasSchemaType::String) => Ok(ValidatedTypeKind::String),
        Some(OasSchemaType::Integer) => Ok(ValidatedTypeKind::Integer(parse_integer_type(
            schema,
            context,
            satay.explicit_integer_type,
        )?)),
        Some(OasSchemaType::Number) => validate_number_type(schema, context),
        Some(OasSchemaType::Boolean) => Ok(ValidatedTypeKind::Bool),
        Some(OasSchemaType::Array) => {
            let items =
                schema
                    .items
                    .as_deref()
                    .ok_or_else(|| ValidationError::MissingArrayItems {
                        context: context.to_owned(),
                    })?;
            Ok(ValidatedTypeKind::Array(Box::new(validate_type_schema(
                document,
                items,
                &format!("{context} items"),
                false,
            )?)))
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

fn validate_number_type(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<ValidatedTypeKind, ValidationError> {
    match schema.format.as_deref() {
        Some("float") => Ok(ValidatedTypeKind::F32),
        Some("double") | None => Ok(ValidatedTypeKind::F64),
        Some(format) => Err(ValidationError::UnsupportedNumberFormat {
            context: context.to_owned(),
            format: format.to_owned(),
        }),
    }
}

fn validation_base_type(kind: &ValidatedTypeKind) -> Option<TypeRef> {
    match kind {
        ValidatedTypeKind::String => Some(TypeRef::String),
        ValidatedTypeKind::Integer(integer_type) => Some(TypeRef::Integer(*integer_type)),
        ValidatedTypeKind::F32 => Some(TypeRef::F32),
        ValidatedTypeKind::F64 => Some(TypeRef::F64),
        ValidatedTypeKind::Bool => Some(TypeRef::Bool),
        ValidatedTypeKind::Array(_) => Some(TypeRef::Array(Box::new(TypeRef::Bool))),
        ValidatedTypeKind::Named(_)
        | ValidatedTypeKind::ParsedString(_)
        | ValidatedTypeKind::ParsedInteger(_)
        | ValidatedTypeKind::Enum(_)
        | ValidatedTypeKind::Range(_) => None,
    }
}

fn validate_struct_properties(
    document: &ResolvedDocument<'_>,
    schema_name: &str,
    schema: &OasObjectSchema,
) -> Result<Vec<ValidatedField>, ValidationError> {
    let context = format!("schema `{schema_name}`");
    reject_keyword(schema.min_properties.is_some(), "minProperties", &context)?;
    reject_keyword(schema.max_properties.is_some(), "maxProperties", &context)?;

    let required = parse_required_set(schema);
    let mut fields = Vec::with_capacity(schema.properties.len());

    for (wire_name, property_schema) in &schema.properties {
        let property_context = format!("property `{schema_name}.{wire_name}`");
        let ty = validate_type_schema(document, property_schema, &property_context, true)?;
        fields.push(ValidatedField {
            wire_name: wire_name.clone(),
            description: schema_description(property_schema),
            treat_error_as_none: ty.treat_error_as_none,
            ty,
            required: required.contains(wire_name),
        });
    }

    Ok(fields)
}

fn parse_required_set(schema: &OasObjectSchema) -> BTreeSet<String> {
    schema.required.iter().cloned().collect()
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

fn validated_enum_variants(
    schema: &OasObjectSchema,
    explicit_variants: &BTreeMap<String, String>,
    context: &str,
) -> Result<Vec<EnumVariant>, ValidationError> {
    let mut used = BTreeSet::from(["Unknown".to_owned()]);

    for rust_name in explicit_variants.values() {
        if rust_name != "Unknown" {
            used.insert(rust_name.clone());
        }
    }

    let mut variants = Vec::with_capacity(schema.enum_values.len());

    for value in &schema.enum_values {
        let Some(wire_name) = value.as_str() else {
            return Err(ValidationError::NonStringEnumValue {
                context: context.to_owned(),
            });
        };
        let rust_name = if let Some(rust_name) = explicit_variants.get(wire_name) {
            if rust_name == "Unknown" {
                continue;
            }
            rust_name.clone()
        } else {
            unique_ident(variant_ident(wire_name), &mut used)
        };
        variants.push(EnumVariant {
            wire_name: wire_name.to_owned(),
            rust_name,
        });
    }

    Ok(variants)
}
