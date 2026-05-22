use crate::ident::{field_ident, type_ident, unique_ident, variant_ident};
use crate::model::{
    Component, ComponentKind, ConstrainedType, EnumVariant, Field, RangeType, TypeRef,
};
use crate::parse::registry::TypeRegistry;
use crate::parse::validate::{
    ValidatedComponent, ValidatedComponentKind, ValidatedDocument, ValidatedField, ValidatedType,
    ValidatedTypeKind,
};

use std::collections::BTreeSet;

pub(super) fn parse_components(
    document: &ValidatedDocument<'_>,
    registry: &mut TypeRegistry,
) -> Vec<Component> {
    document
        .components
        .iter()
        .map(|component| parse_component(component, registry))
        .collect()
}

fn parse_component(component: &ValidatedComponent, registry: &mut TypeRegistry) -> Component {
    let rust_name = type_ident(&component.schema_name);
    let kind = match &component.kind {
        ValidatedComponentKind::Reference(reference) => {
            ComponentKind::Alias(TypeRef::Named(reference.clone()))
        }
        ValidatedComponentKind::Struct(fields) => ComponentKind::Struct(parse_struct_fields(
            &component.schema_name,
            fields,
            registry,
        )),
        ValidatedComponentKind::Type(ty) => {
            parse_component_type(&component.schema_name, &component.description, ty, registry)
        }
    };

    Component {
        rust_name,
        description: component.description.clone(),
        kind,
    }
}

fn parse_component_type(
    schema_name: &str,
    description: &Option<String>,
    ty: &ValidatedType,
    registry: &mut TypeRegistry,
) -> ComponentKind {
    let rust_name = type_ident(schema_name);

    match (&ty.kind, ty.nullable, ty.validation.as_ref()) {
        (ValidatedTypeKind::Enum(variants), false, None) => ComponentKind::Enum(variants.clone()),
        (ValidatedTypeKind::Range(scalar), false, None) => ComponentKind::Range(RangeType {
            rust_name,
            description: description.clone(),
            scalar: *scalar,
        }),
        (_, false, Some(validation)) => ComponentKind::Nutype(ConstrainedType {
            rust_name,
            description: description.clone(),
            inner: parse_type_ref_base(&ty.kind, schema_name, &ty.description, registry),
            validation: validation.clone(),
        }),
        (_, true, Some(validation)) => {
            let hint = format!("{schema_name} value");
            let base = parse_type_ref_base(&ty.kind, schema_name, &ty.description, registry);
            let inner =
                registry.constrained_ref(&hint, ty.description.clone(), base, validation.clone());
            ComponentKind::Alias(TypeRef::Nullable(Box::new(inner)))
        }
        (ValidatedTypeKind::Enum(_) | ValidatedTypeKind::Range(_), true, None) => {
            let hint = format!("{schema_name} value");
            ComponentKind::Alias(parse_type_ref_with_hint(ty, &hint, registry))
        }
        _ => ComponentKind::Alias(parse_type_ref_with_hint(ty, schema_name, registry)),
    }
}

fn parse_struct_fields(
    schema_name: &str,
    fields: &[ValidatedField],
    registry: &mut TypeRegistry,
) -> Vec<Field> {
    let mut used = BTreeSet::new();
    let mut parsed = Vec::with_capacity(fields.len());

    for field in fields {
        parsed.push(Field {
            wire_name: field.wire_name.clone(),
            rust_name: unique_ident(field_ident(&field.wire_name), &mut used),
            description: field.description.clone(),
            ty: parse_type_ref_with_hint(
                &field.ty,
                &format!("{schema_name} {}", field.wire_name),
                registry,
            ),
            required: field.required,
            treat_error_as_none: field.treat_error_as_none,
        });
    }

    parsed
}

pub(super) fn parse_type_ref_with_hint(
    ty: &ValidatedType,
    type_name_hint: &str,
    registry: &mut TypeRegistry,
) -> TypeRef {
    let mut parsed = parse_type_ref_base(&ty.kind, type_name_hint, &ty.description, registry);

    if let Some(validation) = &ty.validation {
        parsed = registry.constrained_ref(
            type_name_hint,
            ty.description.clone(),
            parsed,
            validation.clone(),
        );
    }

    if ty.nullable {
        TypeRef::Nullable(Box::new(parsed))
    } else {
        parsed
    }
}

fn parse_type_ref_base(
    kind: &ValidatedTypeKind,
    type_name_hint: &str,
    description: &Option<String>,
    registry: &mut TypeRegistry,
) -> TypeRef {
    match kind {
        ValidatedTypeKind::Named(rust_name) => TypeRef::Named(rust_name.clone()),
        ValidatedTypeKind::String => TypeRef::String,
        ValidatedTypeKind::ParsedString(parse_as) => TypeRef::ParsedString(*parse_as),
        ValidatedTypeKind::ParsedInteger(parse_as) => TypeRef::ParsedInteger(*parse_as),
        ValidatedTypeKind::Integer(integer_type) => TypeRef::Integer(*integer_type),
        ValidatedTypeKind::F32 => TypeRef::F32,
        ValidatedTypeKind::F64 => TypeRef::F64,
        ValidatedTypeKind::Bool => TypeRef::Bool,
        ValidatedTypeKind::Array(item) => TypeRef::Array(Box::new(parse_type_ref_with_hint(
            item,
            &format!("{type_name_hint} item"),
            registry,
        ))),
        ValidatedTypeKind::Enum(variants) => parse_inline_enum_ref(
            variants.clone(),
            type_name_hint,
            description.clone(),
            registry,
        ),
        ValidatedTypeKind::Range(scalar) => {
            registry.inline_range_ref(type_name_hint, description.clone(), *scalar)
        }
    }
}

fn parse_inline_enum_ref(
    mut variants: Vec<EnumVariant>,
    type_name_hint: &str,
    description: Option<String>,
    registry: &mut TypeRegistry,
) -> TypeRef {
    let default_empty_variant = variant_ident("");
    variants.retain(|variant| {
        !variant.wire_name.is_empty() || variant.rust_name != default_empty_variant
    });

    if variants.is_empty() {
        TypeRef::String
    } else {
        registry.inline_enum_ref(type_name_hint, description, variants)
    }
}
