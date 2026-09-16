//! Validation over the Rust model, independent of frontend syntax.
use crate::error::ValidationError;
use crate::ident::{field_ident, type_ident, unique_ident, variant_ident};
use crate::model::{
    Enum, EnumFallback, EnumVariant, FloatLimit, IntegerLimit, IntegerType, ParameterDefault,
    Validation,
};
use crate::parse::validate::{
    ValidatedComponent, ValidatedComponentKind, ValidatedField, ValidatedOperation, ValidatedType,
    ValidatedTypeKind, ValidatedUnionVariant, ValidatedUnionVariantKind,
};
use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::parse) fn plain_union_branch_shadows(
    previous: &ValidatedUnionVariant,
    current: &ValidatedUnionVariant,
) -> bool {
    if let (
        ValidatedUnionVariantKind::Reference {
            schema_name: previous_schema,
            ..
        },
        ValidatedUnionVariantKind::Reference {
            schema_name: current_schema,
            ..
        },
    ) = (&previous.kind, &current.kind)
    {
        // A repeated reference to the same component — direct or unwrapped from
        // an annotation-only `allOf` wrapper — accepts exactly the payloads of
        // the earlier branch, so the later branch can never deserialize.
        return previous_schema == current_schema;
    }

    let (ValidatedUnionVariantKind::Inline(previous), ValidatedUnionVariantKind::Inline(current)) =
        (&previous.kind, &current.kind)
    else {
        return false;
    };

    inline_plain_union_branch_shadows(previous, current)
}

fn inline_plain_union_branch_shadows(previous: &ValidatedType, current: &ValidatedType) -> bool {
    if is_unconstrained_string_branch(previous) && is_inline_string_branch(current) {
        return true;
    }

    if constrained_string_branch_shadows_enum(previous, current) {
        return true;
    }

    if is_unconstrained_number_branch(previous) && is_inline_number_or_integer_branch(current) {
        return true;
    }

    if let Some(previous_type) = unconstrained_integer_branch(previous)
        && let Some((current_type, current_validation)) = integer_branch(current)
        && integer_branch_range_covers(previous_type, current_type, current_validation)
    {
        return true;
    }

    is_unconstrained_bool_branch(previous) && is_inline_bool_branch(current)
}

fn is_unconstrained_string_branch(ty: &ValidatedType) -> bool {
    matches!(ty.kind, ValidatedTypeKind::String) && ty.validation.is_none()
}

fn is_inline_string_branch(ty: &ValidatedType) -> bool {
    matches!(
        ty.kind,
        ValidatedTypeKind::String | ValidatedTypeKind::Enum(_)
    )
}

fn constrained_string_branch_shadows_enum(
    previous: &ValidatedType,
    current: &ValidatedType,
) -> bool {
    let (
        ValidatedTypeKind::String,
        Some(Validation::String {
            min_length,
            max_length,
            pattern: None,
        }),
    ) = (&previous.kind, previous.validation.as_ref())
    else {
        return false;
    };

    let ValidatedTypeKind::Enum(enum_) = &current.kind else {
        return false;
    };

    enum_.variants.iter().all(|variant| {
        string_value_satisfies_length_bounds(&variant.wire_name, *min_length, *max_length)
    })
}

fn string_value_satisfies_length_bounds(
    value: &str,
    min_length: Option<u64>,
    max_length: Option<u64>,
) -> bool {
    let length = value.chars().count() as u64;

    if let Some(min_length) = min_length
        && length < min_length
    {
        return false;
    }

    if let Some(max_length) = max_length
        && length > max_length
    {
        return false;
    }

    true
}

fn is_unconstrained_number_branch(ty: &ValidatedType) -> bool {
    matches!(ty.kind, ValidatedTypeKind::F32 | ValidatedTypeKind::F64) && ty.validation.is_none()
}

fn is_inline_number_or_integer_branch(ty: &ValidatedType) -> bool {
    matches!(
        ty.kind,
        ValidatedTypeKind::F32 | ValidatedTypeKind::F64 | ValidatedTypeKind::Integer(_)
    )
}

fn unconstrained_integer_branch(ty: &ValidatedType) -> Option<IntegerType> {
    match (&ty.kind, ty.validation.as_ref()) {
        (ValidatedTypeKind::Integer(integer_type), None) => Some(*integer_type),
        _ => None,
    }
}

fn integer_branch(ty: &ValidatedType) -> Option<(IntegerType, Option<&Validation>)> {
    match &ty.kind {
        ValidatedTypeKind::Integer(integer_type) => Some((*integer_type, ty.validation.as_ref())),
        _ => None,
    }
}

fn integer_branch_range_covers(
    previous_type: IntegerType,
    current_type: IntegerType,
    current_validation: Option<&Validation>,
) -> bool {
    let current_min = integer_branch_min(current_type, current_validation);
    let current_max = integer_branch_max(current_type, current_validation);

    previous_type.min_value() <= current_min && previous_type.max_value() >= current_max
}

fn integer_branch_min(integer_type: IntegerType, validation: Option<&Validation>) -> i128 {
    let type_min = integer_type.min_value();
    let Some(Validation::Integer {
        minimum: Some(minimum),
        ..
    }) = validation
    else {
        return type_min;
    };

    type_min.max(effective_integer_min(*minimum))
}

fn integer_branch_max(integer_type: IntegerType, validation: Option<&Validation>) -> i128 {
    let type_max = integer_type.max_value();
    let Some(Validation::Integer {
        maximum: Some(maximum),
        ..
    }) = validation
    else {
        return type_max;
    };

    type_max.min(effective_integer_max(*maximum))
}

fn effective_integer_min(limit: IntegerLimit) -> i128 {
    if limit.exclusive {
        limit.value.saturating_add(1)
    } else {
        limit.value
    }
}

fn effective_integer_max(limit: IntegerLimit) -> i128 {
    if limit.exclusive {
        limit.value.saturating_sub(1)
    } else {
        limit.value
    }
}

fn is_unconstrained_bool_branch(ty: &ValidatedType) -> bool {
    matches!(ty.kind, ValidatedTypeKind::Bool) && ty.validation.is_none()
}

fn is_inline_bool_branch(ty: &ValidatedType) -> bool {
    matches!(ty.kind, ValidatedTypeKind::Bool)
}

pub(in crate::parse) fn inline_union_enum_variant_name(ty: &ValidatedType) -> Option<String> {
    let ValidatedTypeKind::Enum(enum_) = &ty.kind else {
        return None;
    };
    if enum_.variants.len() == 1 {
        enum_
            .variants
            .first()
            .map(|variant| variant.rust_name.clone())
    } else {
        Some("Enum".to_owned())
    }
}

pub(in crate::parse) fn reject_any_of_cycles(
    components: &[ValidatedComponent],
) -> Result<(), ValidationError> {
    let components = components
        .iter()
        .map(|component| (component.schema_name.clone(), component))
        .collect::<BTreeMap<_, _>>();
    let schemas_by_rust_name = components
        .values()
        .map(|component| {
            (
                type_ident(&component.schema_name),
                component.schema_name.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let graph = components
        .values()
        .filter_map(|component| {
            let mut targets = vec![];
            collect_component_union_targets(component, &schemas_by_rust_name, &mut targets);
            (!targets.is_empty()).then(|| (component.schema_name.clone(), targets))
        })
        .collect::<BTreeMap<_, _>>();

    let mut visited = BTreeSet::new();
    for schema_name in components
        .values()
        .filter(|component| component_contains_union(component))
        .map(|component| component.schema_name.as_str())
    {
        let mut stack = vec![];
        visit_any_of_cycle(schema_name, &graph, &mut stack, &mut visited)?;
    }

    Ok(())
}

fn component_contains_union(component: &ValidatedComponent) -> bool {
    match &component.kind {
        ValidatedComponentKind::Reference(_) => false,
        ValidatedComponentKind::Struct(fields) => fields
            .iter()
            .any(|field| field.value.ty().contains_any_of()),
        ValidatedComponentKind::Type(ty) => ty.contains_any_of(),
    }
}

fn collect_component_union_targets(
    component: &ValidatedComponent,
    schemas_by_rust_name: &BTreeMap<String, String>,
    targets: &mut Vec<String>,
) {
    match &component.kind {
        ValidatedComponentKind::Reference(rust_name) => {
            if let Some(schema_name) = schemas_by_rust_name.get(rust_name) {
                targets.push(schema_name.clone());
            }
        }
        ValidatedComponentKind::Struct(fields) => {
            for field in fields {
                collect_type_union_targets(field.value.ty(), schemas_by_rust_name, targets);
            }
        }
        ValidatedComponentKind::Type(ty) => {
            collect_type_union_targets(ty, schemas_by_rust_name, targets);
        }
    }
}

fn collect_type_union_targets(
    ty: &ValidatedType,
    schemas_by_rust_name: &BTreeMap<String, String>,
    targets: &mut Vec<String>,
) {
    match &ty.kind {
        ValidatedTypeKind::AnyOf(union) => {
            for variant in &union.variants {
                match &variant.kind {
                    ValidatedUnionVariantKind::Reference { schema_name, .. } => {
                        targets.push(schema_name.clone());
                    }
                    ValidatedUnionVariantKind::Inline(ty) => {
                        collect_type_union_targets(ty, schemas_by_rust_name, targets);
                    }
                }
            }
        }
        ValidatedTypeKind::Array(item) | ValidatedTypeKind::Map(item) => {
            collect_type_union_targets(item, schemas_by_rust_name, targets);
        }
        ValidatedTypeKind::InlineStruct(fields) => {
            for field in fields {
                collect_type_union_targets(field.value.ty(), schemas_by_rust_name, targets);
            }
        }
        ValidatedTypeKind::Named(rust_name) => {
            if let Some(schema_name) = schemas_by_rust_name.get(rust_name) {
                targets.push(schema_name.clone());
            }
        }
        // Keep these arms explicit so future ValidatedTypeKind variants force a
        // decision about whether they can contain component references.
        ValidatedTypeKind::String
        | ValidatedTypeKind::ParsedString(_)
        | ValidatedTypeKind::Coordinates(_)
        | ValidatedTypeKind::ParsedInteger(_)
        | ValidatedTypeKind::Integer(_)
        | ValidatedTypeKind::F32
        | ValidatedTypeKind::F64
        | ValidatedTypeKind::Bool
        | ValidatedTypeKind::JsonValue
        | ValidatedTypeKind::Enum(_)
        | ValidatedTypeKind::Range(_) => {}
    }
}

fn any_of_cycle_successors(
    schema_name: &str,
    graph: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    graph.get(schema_name).cloned().unwrap_or_default()
}

fn visit_any_of_cycle(
    schema_name: &str,
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
    for target in any_of_cycle_successors(schema_name, graph) {
        visit_any_of_cycle(&target, graph, stack, visited)?;
    }
    stack.pop();
    visited.insert(schema_name.to_owned());

    Ok(())
}

pub(in crate::parse) fn validate_coordinate_uses(
    components: &[ValidatedComponent],
    operations: &[ValidatedOperation],
) -> Result<(), ValidationError> {
    let by_name = components
        .iter()
        .map(|component| (type_ident(&component.schema_name), component))
        .collect::<BTreeMap<_, _>>();
    for component in components {
        check_coordinate_component(
            component,
            true,
            &format!("schema `{}`", component.schema_name),
            &by_name,
            &mut BTreeSet::new(),
        )?;
    }
    for operation in operations {
        let context = format!("operation `{}`", operation.operation_id);
        for parameter in &operation.parameters {
            check_coordinate_type(
                &parameter.ty,
                false,
                &format!("{context} parameter `{}`", parameter.wire_name),
                &by_name,
                &mut BTreeSet::new(),
            )?;
        }
        if let Some(body) = &operation.request_body {
            check_coordinate_type(
                &body.ty,
                false,
                &format!("{context} request body"),
                &by_name,
                &mut BTreeSet::new(),
            )?;
        }
        for response in &operation.responses {
            if let Some(body) = &response.body {
                check_coordinate_type(
                    body,
                    false,
                    &format!("{context} response `{}`", response.status),
                    &by_name,
                    &mut BTreeSet::new(),
                )?;
            }
        }
    }
    Ok(())
}

fn check_coordinate_component(
    component: &ValidatedComponent,
    field_codec: bool,
    context: &str,
    components: &BTreeMap<String, &ValidatedComponent>,
    visited: &mut BTreeSet<(String, bool)>,
) -> Result<(), ValidationError> {
    if !visited.insert((component.schema_name.clone(), field_codec)) {
        return Ok(());
    }
    match &component.kind {
        ValidatedComponentKind::Reference(name) => check_coordinate_type(
            &ValidatedType::named(name.clone()),
            field_codec,
            context,
            components,
            visited,
        ),
        ValidatedComponentKind::Type(ty) => {
            check_coordinate_type(ty, field_codec, context, components, visited)
        }
        ValidatedComponentKind::Struct(fields) => {
            check_coordinate_fields(fields, context, components, visited)
        }
    }
}

fn check_coordinate_fields(
    fields: &[ValidatedField],
    context: &str,
    components: &BTreeMap<String, &ValidatedComponent>,
    visited: &mut BTreeSet<(String, bool)>,
) -> Result<(), ValidationError> {
    for field in fields {
        check_coordinate_type(
            field.value.ty(),
            true,
            &format!("{context} property `{}`", field.wire_name),
            components,
            visited,
        )?;
    }
    Ok(())
}

fn check_coordinate_type(
    ty: &ValidatedType,
    field_codec: bool,
    context: &str,
    components: &BTreeMap<String, &ValidatedComponent>,
    visited: &mut BTreeSet<(String, bool)>,
) -> Result<(), ValidationError> {
    match &ty.kind {
        ValidatedTypeKind::Coordinates(_) if !field_codec => {
            Err(ValidationError::SatayCoordinatesRequireStructField {
                context: context.to_owned(),
            })
        }
        ValidatedTypeKind::Named(name) => {
            if let Some(component) = components.get(name) {
                check_coordinate_component(component, field_codec, context, components, visited)?;
            }
            Ok(())
        }
        ValidatedTypeKind::Array(item) | ValidatedTypeKind::Map(item) => {
            check_coordinate_type(item, false, context, components, visited)
        }
        ValidatedTypeKind::AnyOf(union) => {
            for variant in &union.variants {
                match &variant.kind {
                    ValidatedUnionVariantKind::Reference { type_name, .. } => {
                        check_coordinate_type(
                            &ValidatedType::named(type_name.clone()),
                            false,
                            context,
                            components,
                            visited,
                        )?;
                    }
                    ValidatedUnionVariantKind::Inline(ty) => {
                        check_coordinate_type(ty, false, context, components, visited)?;
                    }
                }
            }
            Ok(())
        }
        ValidatedTypeKind::InlineStruct(fields) => {
            check_coordinate_fields(fields, context, components, visited)
        }
        _ => Ok(()),
    }
}

pub(in crate::parse) fn validate_rust_field_identifier_collisions(
    context: &str,
    fields: &[ValidatedField],
) -> Result<(), ValidationError> {
    let mut normalized = BTreeMap::<String, (String, bool)>::new();
    let mut generated = BTreeMap::<String, String>::new();
    let mut used = BTreeSet::new();

    for field in fields {
        let explicit = field.identifier.is_some();
        let identifier = field
            .identifier
            .as_ref()
            .map(|identifier| identifier.words().join("-"))
            .unwrap_or_else(|| field.wire_name.clone());
        let candidate = field_ident(&identifier);

        if let Some((first_property, first_explicit)) = normalized.get(&candidate)
            && (explicit || *first_explicit)
        {
            return Err(ValidationError::DuplicateSatayIdentifierRustField {
                context: context.to_owned(),
                first_property: first_property.clone(),
                second_property: field.wire_name.clone(),
                rust_name: candidate,
            });
        }

        if explicit {
            if let Some(first_property) = generated.get(&candidate) {
                return Err(ValidationError::DuplicateSatayIdentifierRustField {
                    context: context.to_owned(),
                    first_property: first_property.clone(),
                    second_property: field.wire_name.clone(),
                    rust_name: candidate,
                });
            }
            used.insert(candidate.clone());
            generated.insert(candidate.clone(), field.wire_name.clone());
        } else {
            let rust_name = unique_ident(candidate.clone(), &mut used);
            generated.insert(rust_name, field.wire_name.clone());
        }

        normalized
            .entry(candidate)
            .or_insert_with(|| (field.wire_name.clone(), explicit));
    }

    Ok(())
}

pub(in crate::parse) fn validated_enum(
    enum_values: &[JsonValue],
    explicit_variants: &BTreeMap<String, String>,
    fallback: EnumFallback,
    context: &str,
) -> Result<Enum, ValidationError> {
    let mut used = BTreeSet::new();

    if fallback == EnumFallback::OtherString {
        used.insert("Other".to_owned());
        for (wire_name, rust_name) in explicit_variants {
            if rust_name == "Other" {
                return Err(ValidationError::ReservedSatayEnumVariantName {
                    context: context.to_owned(),
                    wire_name: wire_name.clone(),
                    rust_name: rust_name.clone(),
                });
            }
        }
    }

    for rust_name in explicit_variants.values() {
        used.insert(rust_name.clone());
    }

    let mut variants = Vec::with_capacity(enum_values.len());

    for value in enum_values {
        let Some(wire_name) = value.as_str() else {
            return Err(ValidationError::NonStringEnumValue {
                context: context.to_owned(),
            });
        };
        let rust_name = if let Some(rust_name) = explicit_variants.get(wire_name) {
            rust_name.clone()
        } else {
            unique_ident(variant_ident(wire_name), &mut used)
        };
        variants.push(EnumVariant {
            wire_name: wire_name.to_owned(),
            rust_name,
        });
    }

    Ok(Enum { variants, fallback })
}
pub(in crate::parse) fn parse_parameter_default(
    value: &JsonValue,
    ty: &ValidatedType,
    wire_name: &str,
) -> Result<ParameterDefault, ValidationError> {
    match &ty.kind {
        ValidatedTypeKind::String => Ok(ParameterDefault::String(
            value
                .as_str()
                .ok_or_else(|| {
                    invalid_parameter_default(wire_name, value, "expected a JSON string")
                })?
                .to_owned(),
        )),
        ValidatedTypeKind::Integer(integer_type) => {
            parse_integer_parameter_default(value, *integer_type, wire_name)
        }
        ValidatedTypeKind::F32 => parse_number_parameter_default(value, true, wire_name),
        ValidatedTypeKind::F64 => parse_number_parameter_default(value, false, wire_name),
        ValidatedTypeKind::Bool => Ok(ParameterDefault::Bool(value.as_bool().ok_or_else(
            || invalid_parameter_default(wire_name, value, "expected a JSON boolean"),
        )?)),
        ValidatedTypeKind::Enum(enum_) => parse_enum_parameter_default(value, enum_, wire_name),
        ValidatedTypeKind::ParsedString(_)
        | ValidatedTypeKind::Coordinates(_)
        | ValidatedTypeKind::ParsedInteger(_) => Err(invalid_parameter_default(
            wire_name,
            value,
            "defaults for x-satay parsed parameters are not supported",
        )),
        ValidatedTypeKind::Array(_) => Err(invalid_parameter_default(
            wire_name,
            value,
            "array parameter defaults are not supported",
        )),
        ValidatedTypeKind::Range(_) => Err(invalid_parameter_default(
            wire_name,
            value,
            "range parameter defaults are not supported",
        )),
        ValidatedTypeKind::Named(_)
        | ValidatedTypeKind::Map(_)
        | ValidatedTypeKind::JsonValue
        | ValidatedTypeKind::AnyOf(_)
        | ValidatedTypeKind::InlineStruct(_) => Err(invalid_parameter_default(
            wire_name,
            value,
            "default is not supported for this parameter type",
        )),
    }
}

fn parse_integer_parameter_default(
    value: &JsonValue,
    integer_type: IntegerType,
    wire_name: &str,
) -> Result<ParameterDefault, ValidationError> {
    let integer = parameter_default_integer(value)
        .ok_or_else(|| invalid_parameter_default(wire_name, value, "expected a JSON integer"))?;
    if integer < integer_type.min_value() || integer > integer_type.max_value() {
        return Err(invalid_parameter_default(
            wire_name,
            value,
            format!(
                "value is outside the generated integer range {}..={}",
                integer_type.min_value(),
                integer_type.max_value()
            ),
        ));
    }
    Ok(ParameterDefault::Integer(integer))
}

fn parse_number_parameter_default(
    value: &JsonValue,
    is_f32: bool,
    wire_name: &str,
) -> Result<ParameterDefault, ValidationError> {
    let number = value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| {
            invalid_parameter_default(wire_name, value, "expected a finite JSON number")
        })?;
    if is_f32 {
        let number = number as f32;
        if !number.is_finite() {
            return Err(invalid_parameter_default(
                wire_name,
                value,
                "value is outside the generated f32 range",
            ));
        }
        Ok(ParameterDefault::F32(number))
    } else {
        Ok(ParameterDefault::F64(number))
    }
}

fn parse_enum_parameter_default(
    value: &JsonValue,
    enum_: &Enum,
    wire_name: &str,
) -> Result<ParameterDefault, ValidationError> {
    let wire_value = value.as_str().ok_or_else(|| {
        invalid_parameter_default(wire_name, value, "expected a JSON string enum value")
    })?;
    let default_empty_variant = variant_ident("");
    match enum_
        .variants
        .iter()
        .find(|variant| variant.wire_name == wire_value)
    {
        Some(variant)
            if enum_.variants.len() > 1
                && variant.wire_name.is_empty()
                && variant.rust_name == default_empty_variant =>
        {
            Err(invalid_parameter_default(
                wire_name,
                value,
                "empty enum default cannot be represented by the generated parameter enum",
            ))
        }
        Some(variant) => Ok(ParameterDefault::EnumVariant {
            wire_value: wire_value.to_owned(),
            rust_name: variant.rust_name.clone(),
        }),
        None if enum_.fallback == EnumFallback::OtherString => {
            Ok(ParameterDefault::OpenEnum(wire_value.to_owned()))
        }
        None => Err(invalid_parameter_default(
            wire_name,
            value,
            "value is not a declared enum variant",
        )),
    }
}

fn parameter_default_integer(value: &JsonValue) -> Option<i128> {
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
}

pub(in crate::parse) fn validate_parameter_default_constraints(
    default: &ParameterDefault,
    validation: Option<&Validation>,
) -> Result<(), String> {
    let Some(validation) = validation else {
        return Ok(());
    };

    match (default, validation) {
        (
            ParameterDefault::String(value)
            | ParameterDefault::EnumVariant {
                wire_value: value, ..
            }
            | ParameterDefault::OpenEnum(value),
            Validation::String {
                min_length,
                max_length,
                pattern,
            },
        ) => {
            let length = u64::try_from(value.chars().count()).unwrap_or(u64::MAX);
            if min_length.is_some_and(|minimum| length < minimum) {
                return Err(format!("string length {length} is below minLength"));
            }
            if max_length.is_some_and(|maximum| length > maximum) {
                return Err(format!("string length {length} exceeds maxLength"));
            }
            if let Some(pattern) = pattern {
                let regex = Regex::new(pattern)
                    .map_err(|error| format!("schema pattern is invalid: {error}"))?;
                if !regex.is_match(value) {
                    return Err(format!("string does not match pattern `{pattern}`"));
                }
            }
        }
        (ParameterDefault::Integer(value), Validation::Integer { minimum, maximum }) => {
            if minimum.is_some_and(|limit| {
                if limit.exclusive {
                    *value <= limit.value
                } else {
                    *value < limit.value
                }
            }) {
                return Err("integer is below the schema minimum".to_owned());
            }
            if maximum.is_some_and(|limit| {
                if limit.exclusive {
                    *value >= limit.value
                } else {
                    *value > limit.value
                }
            }) {
                return Err("integer exceeds the schema maximum".to_owned());
            }
        }
        (ParameterDefault::F32(value), Validation::Number { minimum, maximum }) => {
            validate_number_default_constraints(f64::from(*value), *minimum, *maximum, true)?;
        }
        (ParameterDefault::F64(value), Validation::Number { minimum, maximum }) => {
            validate_number_default_constraints(*value, *minimum, *maximum, false)?;
        }
        (_, Validation::Array { .. }) => {
            unreachable!("array parameter defaults are rejected before constraint validation")
        }
        _ => unreachable!("validated parameter default constraints must match the parameter type"),
    }

    Ok(())
}

fn validate_number_default_constraints(
    value: f64,
    minimum: Option<FloatLimit>,
    maximum: Option<FloatLimit>,
    is_f32: bool,
) -> Result<(), String> {
    let generated_value = |value: f64| {
        if is_f32 {
            f64::from(value as f32)
        } else {
            value
        }
    };
    let value = generated_value(value);

    if minimum.is_some_and(|limit| {
        let minimum = generated_value(limit.value);
        if limit.exclusive {
            value <= minimum
        } else {
            value < minimum
        }
    }) {
        return Err("number is below the schema minimum".to_owned());
    }
    if maximum.is_some_and(|limit| {
        let maximum = generated_value(limit.value);
        if limit.exclusive {
            value >= maximum
        } else {
            value > maximum
        }
    }) {
        return Err("number exceeds the schema maximum".to_owned());
    }

    Ok(())
}

pub(in crate::parse) fn invalid_parameter_default(
    wire_name: &str,
    value: &JsonValue,
    reason: impl Into<String>,
) -> ValidationError {
    ValidationError::InvalidParameterDefault {
        wire_name: wire_name.to_owned(),
        value: value.to_string(),
        reason: reason.into(),
    }
}
