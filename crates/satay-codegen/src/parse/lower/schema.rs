use std::collections::BTreeSet;

use oas3::spec::{
    ObjectSchema as OasObjectSchema, Schema as OasSchema, SchemaType as OasSchemaType,
};

use super::super::helpers::{optional_description, schema_description};
use super::super::reference::{
    object_schema, reject_composition, schema_ref, schema_ref_type_name, schema_type_and_nullable,
};
use super::super::registry::TypeRegistry;
use super::super::validate::constraint::{parse_integer_type, parse_validation, reject_keyword};
use super::super::validate::{ValidatedDocument, ValidatedSatay};
use crate::error::ValidationError;
use crate::ident::{field_ident, type_ident, unique_ident, variant_ident};
use crate::model::{
    Component, ComponentKind, ConstrainedType, EnumVariant, Field, ParseAs, RangeType, TypeRef,
};

pub(super) fn parse_components(
    document: &ValidatedDocument<'_>,
    registry: &mut TypeRegistry,
) -> Result<Vec<Component>, ValidationError> {
    let Some(components) = document.resolved.spec.components.as_ref() else {
        return Ok(vec![]);
    };

    let mut parsed = Vec::with_capacity(components.schemas.len());

    for (schema_name, schema) in &components.schemas {
        let rust_name = type_ident(schema_name);
        let description = schema_description(schema);
        let kind = parse_component_kind(schema_name, schema, registry, &document.satay)?;
        parsed.push(Component {
            rust_name,
            description,
            kind,
        });
    }

    Ok(parsed)
}

fn parse_component_kind(
    schema_name: &str,
    schema: &OasSchema,
    registry: &mut TypeRegistry,
    satay: &ValidatedSatay,
) -> Result<ComponentKind, ValidationError> {
    let context = format!("schema `{schema_name}`");

    if let Some(reference) = schema_ref(schema, &context)? {
        return Ok(ComponentKind::Alias(TypeRef::Named(schema_ref_type_name(
            reference,
        )?)));
    }

    let schema = object_schema(schema, &context)?;

    reject_composition(schema, &context)?;

    let (schema_type, nullable) = schema_type_and_nullable(schema, &context)?;

    if !schema.enum_values.is_empty() {
        let variants = parse_string_enum(schema, &context, satay);
        if nullable {
            let inner = registry.inline_enum_ref(
                &format!("{schema_name} value"),
                optional_description(&schema.description),
                variants,
            );
            return Ok(ComponentKind::Alias(TypeRef::Nullable(Box::new(inner))));
        }
        return Ok(ComponentKind::Enum(variants));
    }

    match schema_type {
        Some(OasSchemaType::Object) | None if !schema.properties.is_empty() => Ok(
            ComponentKind::Struct(parse_struct_fields(schema_name, schema, registry, satay)?),
        ),
        Some(
            OasSchemaType::Array
            | OasSchemaType::String
            | OasSchemaType::Integer
            | OasSchemaType::Number
            | OasSchemaType::Boolean,
        ) => parse_component_alias_or_nutype(
            schema_name,
            schema,
            schema_type,
            nullable,
            registry,
            satay,
        ),
        Some(_) | None => {
            unreachable!("validation should reject unsupported component schemas before lowering");
        }
    }
}

fn parse_component_alias_or_nutype(
    schema_name: &str,
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    nullable: bool,
    registry: &mut TypeRegistry,
    satay: &ValidatedSatay,
) -> Result<ComponentKind, ValidationError> {
    let context = format!("schema `{schema_name}`");
    let rust_name = type_ident(schema_name);
    let description = optional_description(&schema.description);
    let satay_schema = satay.schema(schema, &context);
    let parse_as = satay_schema.parse_as;

    if matches!(parse_as, Some(ParseAs::IntegerRange | ParseAs::NumberRange)) {
        if schema_type != Some(OasSchemaType::String) {
            unreachable!(
                "validation should reject incompatible x-satay range parse-as before lowering for {context}"
            );
        }

        let scalar = satay_schema
            .range_scalar
            .expect("validated x-satay range scalar missing during component lowering");

        if nullable {
            let inner =
                registry.inline_range_ref(&format!("{schema_name} value"), description, scalar);
            return Ok(ComponentKind::Alias(TypeRef::Nullable(Box::new(inner))));
        }

        return Ok(ComponentKind::Range(RangeType {
            rust_name,
            description,
            scalar,
        }));
    }

    let base = parse_type_ref_base(
        schema,
        schema_type,
        &context,
        registry,
        Some(schema_name),
        satay,
    )?;

    let validation = parse_validation(schema, &base, &context)?;

    match (validation, nullable) {
        (Some(validation), false) => Ok(ComponentKind::Nutype(ConstrainedType {
            rust_name,
            description,
            inner: base,
            validation,
        })),
        (Some(validation), true) => {
            let inner = registry.constrained_ref(
                &format!("{schema_name} value"),
                description,
                base,
                validation,
            );
            Ok(ComponentKind::Alias(TypeRef::Nullable(Box::new(inner))))
        }
        (None, false) => Ok(ComponentKind::Alias(base)),
        (None, true) => Ok(ComponentKind::Alias(TypeRef::Nullable(Box::new(base)))),
    }
}

fn parse_string_enum(
    schema: &OasObjectSchema,
    context: &str,
    satay: &ValidatedSatay,
) -> Vec<EnumVariant> {
    let (schema_type, _) = validated_schema_type_and_nullable(schema, context);

    if let Some(kind) = schema_type
        && kind != OasSchemaType::String
    {
        unreachable!("validation should reject unsupported enum types before lowering");
    }

    if schema.enum_values.is_empty() {
        unreachable!("validation should reject empty enums before lowering");
    }

    let mut wire_names = Vec::with_capacity(schema.enum_values.len());

    for value in &schema.enum_values {
        let Some(value) = value.as_str() else {
            unreachable!("validation should reject non-string enum values before lowering");
        };
        wire_names.push(value);
    }

    let explicit_variants = &satay.schema(schema, context).enum_variants;
    let mut used = BTreeSet::from(["Unknown".to_owned()]);

    for rust_name in explicit_variants.values() {
        if rust_name != "Unknown" {
            used.insert(rust_name.clone());
        }
    }

    let mut variants = Vec::with_capacity(schema.enum_values.len());

    for wire_name in wire_names {
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

    variants
}

fn parse_struct_fields(
    schema_name: &str,
    schema: &OasObjectSchema,
    registry: &mut TypeRegistry,
    satay: &ValidatedSatay,
) -> Result<Vec<Field>, ValidationError> {
    let context = format!("schema `{schema_name}`");
    let required = parse_required_set(schema);

    reject_keyword(schema.min_properties.is_some(), "minProperties", &context)?;
    reject_keyword(schema.max_properties.is_some(), "maxProperties", &context)?;

    if schema.properties.is_empty() {
        unreachable!("validation should reject object schemas without properties before lowering");
    }

    let mut used = BTreeSet::new();
    let mut fields = Vec::with_capacity(schema.properties.len());

    for (wire_name, property_schema) in &schema.properties {
        let rust_name = unique_ident(field_ident(wire_name), &mut used);
        let description = schema_description(property_schema);
        let ty = parse_type_ref(
            property_schema,
            &format!("property `{schema_name}.{wire_name}`"),
            registry,
            Some(&format!("{schema_name} {wire_name}")),
            satay,
        )?;
        let treat_error_as_none = satay.treat_error_as_none(
            property_schema,
            &format!("property `{schema_name}.{wire_name}`"),
        );
        fields.push(Field {
            wire_name: wire_name.clone(),
            rust_name,
            description,
            ty,
            required: required.contains(wire_name),
            treat_error_as_none,
        });
    }

    Ok(fields)
}

fn parse_required_set(schema: &OasObjectSchema) -> BTreeSet<String> {
    schema.required.iter().cloned().collect()
}

pub(super) fn parse_type_ref(
    schema: &OasSchema,
    context: &str,
    registry: &mut TypeRegistry,
    type_name_hint: Option<&str>,
    satay: &ValidatedSatay,
) -> Result<TypeRef, ValidationError> {
    if let Some(reference) = schema_ref(schema, context)? {
        return Ok(TypeRef::Named(schema_ref_type_name(reference)?));
    }

    let schema = object_schema(schema, context)?;

    reject_composition(schema, context)?;

    let description = optional_description(&schema.description);
    let (schema_type, nullable) = schema_type_and_nullable(schema, context)?;
    let base = parse_type_ref_base(
        schema,
        schema_type,
        context,
        registry,
        type_name_hint,
        satay,
    )?;

    let validation = parse_validation(schema, &base, context)?;
    let ty = if let Some(validation) = validation {
        registry.constrained_ref(
            type_name_hint.unwrap_or(context),
            description,
            base,
            validation,
        )
    } else {
        base
    };

    if nullable {
        Ok(TypeRef::Nullable(Box::new(ty)))
    } else {
        Ok(ty)
    }
}

fn parse_type_ref_base(
    schema: &OasObjectSchema,
    schema_type: Option<OasSchemaType>,
    context: &str,
    registry: &mut TypeRegistry,
    type_name_hint: Option<&str>,
    satay: &ValidatedSatay,
) -> Result<TypeRef, ValidationError> {
    let description = optional_description(&schema.description);
    let satay_schema = satay.schema(schema, context);
    let parse_as = satay_schema.parse_as;
    let integer_type = satay_schema.integer_type;

    if let Some(parse_as) = parse_as {
        match (schema_type, parse_as) {
            (Some(OasSchemaType::String), ParseAs::IntegerRange | ParseAs::NumberRange) => {
                let scalar = satay_schema
                    .range_scalar
                    .expect("validated x-satay range scalar missing during type lowering");
                return Ok(registry.inline_range_ref(
                    type_name_hint.unwrap_or(context),
                    description,
                    scalar,
                ));
            }
            (Some(OasSchemaType::String), parse_as) => return Ok(TypeRef::ParsedString(parse_as)),
            (Some(OasSchemaType::Integer), ParseAs::Bool) => {
                return Ok(TypeRef::ParsedInteger(parse_as));
            }
            _ => {
                unreachable!(
                    "validation should reject incompatible x-satay parse-as before lowering for {context}"
                );
            }
        }
    }

    if !schema.enum_values.is_empty() {
        let mut variants = parse_string_enum(schema, context, satay);
        let default_empty_variant = variant_ident("");
        variants.retain(|v| !v.wire_name.is_empty() || v.rust_name != default_empty_variant);
        if variants.is_empty() {
            return Ok(TypeRef::String);
        }
        let name_hint = type_name_hint.unwrap_or(context);
        return Ok(registry.inline_enum_ref(
            name_hint,
            optional_description(&schema.description),
            variants,
        ));
    }

    match schema_type {
        Some(OasSchemaType::String) => Ok(TypeRef::String),
        Some(OasSchemaType::Integer) => Ok(TypeRef::Integer(parse_integer_type(
            schema,
            context,
            integer_type,
        )?)),
        Some(OasSchemaType::Number) => match schema.format.as_deref() {
            Some("float") => Ok(TypeRef::F32),
            Some("double") | None => Ok(TypeRef::F64),
            Some(_) => {
                unreachable!("validation should reject unsupported number formats before lowering");
            }
        },
        Some(OasSchemaType::Boolean) => Ok(TypeRef::Bool),
        Some(OasSchemaType::Array) => {
            let items = schema
                .items
                .as_deref()
                .expect("validation should require array items before lowering");
            let item_name_hint = type_name_hint.map(|name| format!("{name} item"));
            Ok(TypeRef::Array(Box::new(parse_type_ref(
                items,
                &format!("{context} items"),
                registry,
                item_name_hint.as_deref(),
                satay,
            )?)))
        }
        Some(OasSchemaType::Object) | None if !schema.properties.is_empty() => {
            unreachable!("validation should reject inline object schemas before lowering");
        }
        Some(OasSchemaType::Object) => {
            unreachable!("validation should reject map object schemas before lowering");
        }
        Some(_) => {
            unreachable!("validation should reject unsupported schema types before lowering");
        }
        None => {
            unreachable!("validation should reject missing schema types before lowering");
        }
    }
}

fn validated_schema_type_and_nullable(
    schema: &OasObjectSchema,
    context: &str,
) -> (Option<OasSchemaType>, bool) {
    schema_type_and_nullable(schema, context).unwrap_or_else(|error| {
        unreachable!("validated schema type failed during lowering for {context}: {error:?}")
    })
}
