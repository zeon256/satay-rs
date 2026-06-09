use std::collections::{BTreeMap, BTreeSet};

use oas3::spec::{
    ObjectSchema as OasObjectSchema, Schema as OasSchema, SchemaType as OasSchemaType,
};

use super::super::helpers::{optional_description, schema_description};
use super::super::reference::{
    object_schema, reject_one_of_all_of, schema_ref, schema_ref_type_name,
    schema_type_and_nullable, schema_type_wire,
};
use super::super::resolve::{ResolvedDocument, refs::local_ref_name};
use super::constraint::{parse_integer_type, parse_validation, reject_keyword};
use super::satay::{
    ValidatedParseAs, ValidatedSataySchema, validate_component_enum_satay,
    validate_type_enum_satay, validate_type_satay,
};
use super::{
    ValidatedComponent, ValidatedComponentKind, ValidatedField, ValidatedType, ValidatedTypeKind,
    ValidatedUnionVariant,
};
use crate::error::ValidationError;
use crate::ident::{unique_ident, variant_ident};
use crate::model::{EnumVariant, ParseAs, TypeRef};

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

    reject_any_of_cycles(&parsed)?;

    Ok(parsed)
}

pub(super) fn validate_type_schema(
    document: &ResolvedDocument<'_>,
    schema: &OasSchema,
    context: &str,
    allow_treat_error_as_none: bool,
) -> Result<ValidatedType, ValidationError> {
    if let Some(reference) = schema_ref(schema, context)? {
        let description = match schema_description(schema) {
            Some(description) => Some(description),
            None => referenced_schema_description(document, reference)?,
        };
        let mut ty = ValidatedType::named(schema_ref_type_name(reference)?);
        ty.description = description;
        return Ok(ty);
    }

    let schema = object_schema(schema, context)?;
    reject_one_of_all_of(schema, context)?;
    if schema_is_any_of_union(schema) {
        return validate_any_of_type_schema(schema, context);
    }
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

pub(super) fn schema_uses_any_of(
    document: &ResolvedDocument<'_>,
    schema: &OasSchema,
) -> Result<bool, ValidationError> {
    let mut visited = BTreeSet::new();
    schema_uses_any_of_inner(document, schema, &mut visited)
}

fn schema_uses_any_of_inner(
    document: &ResolvedDocument<'_>,
    schema: &OasSchema,
    visited: &mut BTreeSet<String>,
) -> Result<bool, ValidationError> {
    if let Some(reference) = schema_ref(schema, "anyOf parameter validation")? {
        let name = local_ref_name(reference, "schemas")?;
        if !visited.insert(name.clone()) {
            return Ok(false);
        }
        let target = document
            .spec
            .components
            .as_ref()
            .and_then(|components| components.schemas.get(&name))
            .ok_or(ValidationError::MissingJsonPointerToken { token: name })?;
        return schema_uses_any_of_inner(document, target, visited);
    }

    let schema = object_schema(schema, "anyOf parameter validation")?;
    if !schema.any_of.is_empty() {
        return Ok(true);
    }

    if let Some(items) = schema.items.as_deref()
        && schema_uses_any_of_inner(document, items, visited)?
    {
        return Ok(true);
    }

    Ok(false)
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
        reject_one_of_all_of(schema, &context)?;
        if schema_is_any_of_union(schema) {
            ValidatedComponentKind::Type(validate_any_of_type_schema(schema, &context)?)
        } else {
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
        }
    };

    Ok(ValidatedComponent {
        schema_name: schema_name.to_owned(),
        description,
        kind,
    })
}

fn schema_is_any_of_union(schema: &OasObjectSchema) -> bool {
    if !schema.any_of.is_empty() {
        return true;
    }

    schema_is_empty_any_of_shape(schema)
}

fn schema_is_empty_any_of_shape(schema: &OasObjectSchema) -> bool {
    if !schema.one_of.is_empty() || !schema.all_of.is_empty() {
        return false;
    }

    reject_any_of_sibling_keywords(schema, "").is_ok()
}

fn validate_any_of_type_schema(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<ValidatedType, ValidationError> {
    reject_any_of_sibling_keywords(schema, context)?;

    if schema.any_of.is_empty() {
        return Err(ValidationError::EmptyAnyOf {
            context: context.to_owned(),
        });
    }

    let mut used = BTreeSet::new();
    let mut variants = Vec::with_capacity(schema.any_of.len());

    for (index, branch) in schema.any_of.iter().enumerate() {
        let Some(reference) = schema_ref(branch, context)? else {
            return Err(ValidationError::UnsupportedAnyOfBranch {
                context: context.to_owned(),
                index,
            });
        };
        let schema_name = local_ref_name(reference, "schemas")?;
        let type_name = schema_ref_type_name(reference)?;
        variants.push(ValidatedUnionVariant {
            rust_name: unique_ident(type_name.clone(), &mut used),
            type_name,
            schema_name,
        });
    }

    Ok(ValidatedType {
        kind: ValidatedTypeKind::AnyOf(variants),
        nullable: false,
        validation: None,
        description: optional_description(&schema.description),
        treat_error_as_none: false,
    })
}

fn reject_any_of_sibling_keywords(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    for (keyword, present) in [
        ("type", schema.schema_type.is_some()),
        ("enum", !schema.enum_values.is_empty()),
        ("const", schema.const_value.is_some()),
        ("items", schema.items.is_some()),
        ("prefixItems", !schema.prefix_items.is_empty()),
        ("properties", !schema.properties.is_empty()),
        (
            "additionalProperties",
            schema.additional_properties.is_some(),
        ),
        ("multipleOf", schema.multiple_of.is_some()),
        ("maximum", schema.maximum.is_some()),
        ("exclusiveMaximum", schema.exclusive_maximum.is_some()),
        ("minimum", schema.minimum.is_some()),
        ("exclusiveMinimum", schema.exclusive_minimum.is_some()),
        ("maxLength", schema.max_length.is_some()),
        ("minLength", schema.min_length.is_some()),
        ("pattern", schema.pattern.is_some()),
        ("maxItems", schema.max_items.is_some()),
        ("minItems", schema.min_items.is_some()),
        ("uniqueItems", schema.unique_items.is_some()),
        ("maxProperties", schema.max_properties.is_some()),
        ("minProperties", schema.min_properties.is_some()),
        ("required", !schema.required.is_empty()),
        ("format", schema.format.is_some()),
        ("discriminator", schema.discriminator.is_some()),
    ] {
        if present {
            return Err(ValidationError::UnsupportedAnyOfSiblingKeyword {
                context: context.to_owned(),
                keyword: keyword.to_owned(),
            });
        }
    }

    if let Some(keyword) = schema.extensions.keys().next() {
        return Err(ValidationError::UnsupportedAnyOfSiblingKeyword {
            context: context.to_owned(),
            keyword: format!("x-{keyword}"),
        });
    }

    Ok(())
}

fn reject_any_of_cycles(components: &[ValidatedComponent]) -> Result<(), ValidationError> {
    let components = components
        .iter()
        .map(|component| (component.schema_name.clone(), component))
        .collect::<BTreeMap<_, _>>();
    let graph = components
        .values()
        .filter_map(|component| {
            let mut targets = vec![];
            collect_component_any_of_targets(component, &mut targets);
            (!targets.is_empty()).then(|| (component.schema_name.clone(), targets))
        })
        .collect::<BTreeMap<_, _>>();

    let mut visited = BTreeSet::new();
    for schema_name in graph.keys() {
        let mut stack = vec![];
        visit_any_of_cycle(schema_name, &components, &graph, &mut stack, &mut visited)?;
    }

    Ok(())
}

fn collect_component_any_of_targets(component: &ValidatedComponent, targets: &mut Vec<String>) {
    match &component.kind {
        ValidatedComponentKind::Reference(_) => {}
        ValidatedComponentKind::Struct(fields) => {
            for field in fields {
                collect_type_any_of_targets(&field.ty, targets);
            }
        }
        ValidatedComponentKind::Type(ty) => collect_type_any_of_targets(ty, targets),
    }
}

fn collect_type_any_of_targets(ty: &ValidatedType, targets: &mut Vec<String>) {
    match &ty.kind {
        ValidatedTypeKind::AnyOf(variants) => {
            targets.extend(variants.iter().map(|variant| variant.schema_name.clone()));
        }
        ValidatedTypeKind::Array(item) => collect_type_any_of_targets(item, targets),
        // Keep these arms explicit so future ValidatedTypeKind variants force a
        // decision about whether they can contain nested anyOf schemas.
        ValidatedTypeKind::Named(_)
        | ValidatedTypeKind::String
        | ValidatedTypeKind::ParsedString(_)
        | ValidatedTypeKind::ParsedInteger(_)
        | ValidatedTypeKind::Integer(_)
        | ValidatedTypeKind::F32
        | ValidatedTypeKind::F64
        | ValidatedTypeKind::Bool
        | ValidatedTypeKind::Enum(_)
        | ValidatedTypeKind::Range(_) => {}
    }
}

fn any_of_cycle_successors(
    schema_name: &str,
    components: &BTreeMap<String, &ValidatedComponent>,
    graph: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    match components.get(schema_name).map(|component| &component.kind) {
        Some(ValidatedComponentKind::Reference(target)) => {
            any_of_cycle_successors(target, components, graph)
        }
        _ => graph.get(schema_name).cloned().unwrap_or_default(),
    }
}

fn visit_any_of_cycle(
    schema_name: &str,
    components: &BTreeMap<String, &ValidatedComponent>,
    graph: &BTreeMap<String, Vec<String>>,
    stack: &mut Vec<String>,
    visited: &mut BTreeSet<String>,
) -> Result<(), ValidationError> {
    if let Some(index) = stack.iter().position(|visited| visited == schema_name) {
        return Err(ValidationError::RecursiveAnyOf {
            context: format!("schema `{}`", stack[index]),
            schema: schema_name.to_owned(),
        });
    }

    if visited.contains(schema_name) {
        return Ok(());
    }

    stack.push(schema_name.to_owned());
    for target in any_of_cycle_successors(schema_name, components, graph) {
        visit_any_of_cycle(&target, components, graph, stack, visited)?;
    }
    stack.pop();
    visited.insert(schema_name.to_owned());

    Ok(())
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
        Some(OasSchemaType::String) => validate_string_type(schema),
        Some(OasSchemaType::Integer) => {
            if schema.format.as_deref() == Some("unixtime") {
                Ok(ValidatedTypeKind::ParsedInteger(ParseAs::UnixTime))
            } else {
                Ok(ValidatedTypeKind::Integer(parse_integer_type(
                    schema,
                    context,
                    satay.explicit_integer_type,
                )?))
            }
        }
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

fn validate_string_type(schema: &OasObjectSchema) -> Result<ValidatedTypeKind, ValidationError> {
    match schema.format.as_deref() {
        Some("unixtime") => Ok(ValidatedTypeKind::ParsedString(ParseAs::UnixTime)),
        _ => Ok(ValidatedTypeKind::String),
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
        | ValidatedTypeKind::AnyOf(_)
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
            description: ty.description.clone(),
            treat_error_as_none: ty.treat_error_as_none,
            ty,
            required: required.contains(wire_name),
        });
    }

    Ok(fields)
}

fn referenced_schema_description(
    document: &ResolvedDocument<'_>,
    reference: &str,
) -> Result<Option<String>, ValidationError> {
    let mut visited = BTreeSet::new();
    referenced_schema_description_inner(document, reference, &mut visited)
}

fn referenced_schema_description_inner(
    document: &ResolvedDocument<'_>,
    reference: &str,
    visited: &mut BTreeSet<String>,
) -> Result<Option<String>, ValidationError> {
    if !visited.insert(reference.to_owned()) {
        return Ok(None);
    }

    let name = local_ref_name(reference, "schemas")?;
    let target = document
        .spec
        .components
        .as_ref()
        .and_then(|components| components.schemas.get(&name))
        .ok_or(ValidationError::MissingJsonPointerToken { token: name })?;

    if let Some(description) = schema_description(target) {
        return Ok(Some(description));
    }

    let Some(reference) = schema_ref(target, "referenced schema description")? else {
        return Ok(None);
    };
    referenced_schema_description_inner(document, reference, visited)
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
