use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use oas3::spec::{
    Discriminator as OasDiscriminator, ObjectSchema as OasObjectSchema, Schema as OasSchema,
    SchemaType as OasSchemaType, SchemaTypeSet as OasSchemaTypeSet,
};

use super::super::helpers::{optional_description, schema_description};
use super::super::reference::{
    object_schema, reject_one_of, schema_ref, schema_ref_type_name, schema_type_and_nullable,
    schema_type_wire,
};
use super::super::resolve::{ResolvedDocument, refs::local_ref_name};
use super::constraint::{parse_integer_type, parse_validation, reject_keyword};
use super::satay::{
    ValidatedParseAs, ValidatedSataySchema, validate_component_enum_satay,
    validate_type_enum_satay, validate_type_satay,
};
use super::{
    ValidatedComponent, ValidatedComponentKind, ValidatedField, ValidatedType, ValidatedTypeKind,
    ValidatedUnion, ValidatedUnionTag, ValidatedUnionVariant, ValidatedUnionVariantKind,
};
use crate::error::ValidationError;
use crate::ident::{type_ident, unique_ident, variant_ident};
use crate::model::{Enum, EnumFallback, EnumVariant, ParseAs, TypeRef};

pub(super) fn validate_components(
    document: &ResolvedDocument<'_>,
) -> Result<Vec<ValidatedComponent>, ValidationError> {
    let Some(components) = document.spec.components.as_ref() else {
        return Ok(vec![]);
    };

    let mut parsed = Vec::with_capacity(components.schemas.len());

    for (schema_name, schema) in &components.schemas {
        let mut stack = vec![];
        parsed.push(validate_component_schema(
            document,
            schema_name,
            schema,
            &mut stack,
        )?);
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
    if schema_is_union(schema) {
        return validate_union_type_schema(document, schema, context);
    }
    reject_one_of(schema, context)?;
    if !schema.all_of.is_empty() {
        return Err(ValidationError::UnsupportedComposition {
            context: context.to_owned(),
            keyword: "allOf",
        });
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
    if !schema.any_of.is_empty() || !schema.one_of.is_empty() || schema.discriminator.is_some() {
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
    stack: &mut Vec<String>,
) -> Result<ValidatedComponent, ValidationError> {
    let context = format!("schema `{schema_name}`");
    let description = schema_description(schema);
    let kind = if let Some(reference) = schema_ref(schema, &context)? {
        ValidatedComponentKind::Reference(schema_ref_type_name(reference)?)
    } else {
        let schema = object_schema(schema, &context)?;
        if schema_is_union(schema) {
            ValidatedComponentKind::Type(validate_union_type_schema(document, schema, &context)?)
        } else if !schema.all_of.is_empty() {
            ValidatedComponentKind::Struct(validate_all_of_struct_properties(
                document,
                schema_name,
                schema,
                stack,
            )?)
        } else {
            let (schema_type, nullable) = schema_type_and_nullable(schema, &context)?;

            if !schema.enum_values.is_empty() {
                validate_enum_shape(schema, schema_type, &context)?;
                let validated_satay = validate_component_enum_satay(schema, &context)?;
                ValidatedComponentKind::Type(ValidatedType {
                    kind: ValidatedTypeKind::Enum(validated_enum(
                        schema,
                        &validated_satay.enum_variants,
                        EnumFallback::None,
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

fn schema_is_union(schema: &OasObjectSchema) -> bool {
    if !schema.any_of.is_empty() || !schema.one_of.is_empty() || schema.discriminator.is_some() {
        return true;
    }

    schema_is_empty_any_of_shape(schema)
}

/// True when a schema has no composition branches but also no unsupported siblings.
///
/// Empty component `allOf: []` matches this shape because `all_of` is empty, so it is
/// routed through the `anyOf` validator and rejected with `EmptyAnyOf`.
fn schema_is_empty_any_of_shape(schema: &OasObjectSchema) -> bool {
    if !schema.one_of.is_empty() || !schema.all_of.is_empty() {
        return false;
    }

    reject_any_of_sibling_keywords(schema, "").is_ok()
}

fn validate_union_type_schema(
    document: &ResolvedDocument<'_>,
    schema: &OasObjectSchema,
    context: &str,
) -> Result<ValidatedType, ValidationError> {
    if let Some(open_enum) = validate_open_string_enum_any_of(schema, context)? {
        return Ok(open_enum);
    }

    let union = if let Some(discriminator) = schema.discriminator.as_ref() {
        validate_discriminator_union(document, schema, discriminator, context)?
    } else if !schema.one_of.is_empty() && schema.any_of.is_empty() {
        validate_plain_one_of_union(schema, context)?
    } else {
        validate_plain_any_of_union(schema, context)?
    };

    Ok(ValidatedType {
        kind: ValidatedTypeKind::AnyOf(union),
        nullable: false,
        validation: None,
        description: optional_description(&schema.description),
        treat_error_as_none: false,
    })
}

fn validate_open_string_enum_any_of(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<Option<ValidatedType>, ValidationError> {
    if schema.discriminator.is_some() || !schema.one_of.is_empty() || schema.any_of.len() != 2 {
        return Ok(None);
    }

    reject_any_of_sibling_keywords(schema, context)?;

    let mut has_open_string = false;
    let mut enum_branch = None;

    for branch in &schema.any_of {
        if open_string_any_of_branch_is_unconstrained_string(branch, context)? {
            has_open_string = true;
            continue;
        }

        if let Some(ty) = validate_open_string_enum_any_of_branch(branch, context)? {
            enum_branch = Some(ty);
            continue;
        }

        return Ok(None);
    }

    let Some(enum_branch) = enum_branch else {
        return Ok(None);
    };
    if !has_open_string {
        return Ok(None);
    }

    Ok(Some(ValidatedType {
        description: optional_description(&schema.description),
        ..enum_branch
    }))
}

fn open_string_any_of_branch_is_unconstrained_string(
    branch: &OasSchema,
    context: &str,
) -> Result<bool, ValidationError> {
    let Some(schema) = open_string_any_of_object_branch(branch, context)? else {
        return Ok(false);
    };
    let (schema_type, nullable) = schema_type_and_nullable(schema, context)?;

    Ok(!nullable
        && schema_type == Some(OasSchemaType::String)
        && schema.enum_values.is_empty()
        && schema.const_value.is_none()
        && schema.format.is_none()
        && schema.items.is_none()
        && schema.prefix_items.is_empty()
        && schema.properties.is_empty()
        && schema.additional_properties.is_none()
        && schema.multiple_of.is_none()
        && schema.maximum.is_none()
        && schema.exclusive_maximum.is_none()
        && schema.minimum.is_none()
        && schema.exclusive_minimum.is_none()
        && schema.max_length.is_none()
        && schema.min_length.is_none()
        && schema.pattern.is_none()
        && schema.max_items.is_none()
        && schema.min_items.is_none()
        && schema.unique_items.is_none()
        && schema.max_properties.is_none()
        && schema.min_properties.is_none()
        && schema.required.is_empty()
        && schema.all_of.is_empty()
        && schema.any_of.is_empty()
        && schema.one_of.is_empty()
        && schema.discriminator.is_none()
        && unsupported_union_extension(schema).is_none())
}

fn validate_open_string_enum_any_of_branch(
    branch: &OasSchema,
    context: &str,
) -> Result<Option<ValidatedType>, ValidationError> {
    let Some(schema) = open_string_any_of_object_branch(branch, context)? else {
        return Ok(None);
    };
    let (schema_type, nullable) = schema_type_and_nullable(schema, context)?;
    if nullable || schema_type != Some(OasSchemaType::String) || schema.enum_values.is_empty() {
        return Ok(None);
    }

    validate_enum_shape(schema, schema_type, context)?;
    let explicit_variants = validate_type_enum_satay(schema, context)?;
    let enum_ = validated_enum(
        schema,
        &explicit_variants,
        EnumFallback::OtherString,
        context,
    )?;

    Ok(Some(ValidatedType {
        kind: ValidatedTypeKind::Enum(enum_),
        nullable: false,
        validation: None,
        description: optional_description(&schema.description),
        treat_error_as_none: false,
    }))
}

fn open_string_any_of_object_branch<'a>(
    branch: &'a OasSchema,
    context: &str,
) -> Result<Option<&'a OasObjectSchema>, ValidationError> {
    if schema_ref(branch, context)?.is_some() {
        return Ok(None);
    }

    match object_schema(branch, context) {
        Ok(schema) => Ok(Some(schema)),
        Err(_) => Ok(None),
    }
}

fn validate_plain_any_of_union(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<ValidatedUnion, ValidationError> {
    reject_any_of_sibling_keywords(schema, context)?;

    if schema.any_of.is_empty() {
        return Err(ValidationError::EmptyAnyOf {
            context: context.to_owned(),
        });
    }

    let mut used = BTreeSet::new();
    let mut variants = Vec::with_capacity(schema.any_of.len());

    for (index, branch) in schema.any_of.iter().enumerate() {
        variants.push(validate_plain_union_branch(
            branch,
            context,
            index,
            PlainUnionKeyword::AnyOf,
            &mut used,
        )?);
    }

    Ok(ValidatedUnion {
        variants,
        tag: None,
    })
}

fn validate_plain_one_of_union(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<ValidatedUnion, ValidationError> {
    reject_plain_one_of_sibling_keywords(schema, context)?;

    if schema.one_of.is_empty() {
        return Err(ValidationError::EmptyAnyOf {
            context: context.to_owned(),
        });
    }

    let mut used = BTreeSet::new();
    let mut variants = Vec::with_capacity(schema.one_of.len());

    for (index, branch) in schema.one_of.iter().enumerate() {
        variants.push(validate_plain_union_branch(
            branch,
            context,
            index,
            PlainUnionKeyword::OneOf,
            &mut used,
        )?);
    }

    Ok(ValidatedUnion {
        variants,
        tag: None,
    })
}

fn validate_plain_union_branch(
    branch: &OasSchema,
    context: &str,
    index: usize,
    keyword: PlainUnionKeyword,
    used: &mut BTreeSet<String>,
) -> Result<ValidatedUnionVariant, ValidationError> {
    if let Some(reference) = schema_ref(branch, context)? {
        let schema_name = local_ref_name(reference, "schemas")?;
        let type_name = schema_ref_type_name(reference)?;
        return Ok(ValidatedUnionVariant {
            rust_name: unique_ident(type_name.clone(), used),
            kind: ValidatedUnionVariantKind::Reference {
                type_name,
                schema_name,
            },
            tag_value: None,
        });
    }

    let ty = validate_inline_union_enum_branch(branch, context, index, keyword)?;
    let rust_name = inline_union_enum_variant_name(&ty)
        .expect("validated inline union enum branch has at least one variant");
    Ok(ValidatedUnionVariant {
        rust_name: unique_ident(rust_name, used),
        kind: ValidatedUnionVariantKind::Inline(ty),
        tag_value: None,
    })
}

fn validate_inline_union_enum_branch(
    branch: &OasSchema,
    context: &str,
    index: usize,
    keyword: PlainUnionKeyword,
) -> Result<ValidatedType, ValidationError> {
    let schema =
        object_schema(branch, context).map_err(|_| keyword.branch_error(context, index))?;
    let (schema_type, nullable) = schema_type_and_nullable(schema, context)
        .map_err(|_| keyword.branch_error(context, index))?;

    if nullable || schema_type != Some(OasSchemaType::String) || schema.enum_values.is_empty() {
        return Err(keyword.branch_error(context, index));
    }

    validate_enum_shape(schema, schema_type, context)
        .map_err(|_| keyword.branch_error(context, index))?;
    let explicit_variants = validate_type_enum_satay(schema, context)
        .map_err(|_| keyword.branch_error(context, index))?;
    let enum_ = validated_enum(schema, &explicit_variants, EnumFallback::None, context)
        .map_err(|_| keyword.branch_error(context, index))?;

    Ok(ValidatedType {
        kind: ValidatedTypeKind::Enum(enum_),
        nullable,
        validation: None,
        description: optional_description(&schema.description),
        treat_error_as_none: false,
    })
}

fn inline_union_enum_variant_name(ty: &ValidatedType) -> Option<String> {
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

fn validate_discriminator_union(
    document: &ResolvedDocument<'_>,
    schema: &OasObjectSchema,
    discriminator: &OasDiscriminator,
    context: &str,
) -> Result<ValidatedUnion, ValidationError> {
    reject_discriminator_union_sibling_keywords(schema, context)?;

    let (keyword, branches) = discriminator_union_branches(schema, context)?;
    let branch_refs = validate_discriminator_branch_refs(branches, keyword, context)?;
    let tag_values = validate_discriminator_mapping(discriminator, &branch_refs, context)?;

    let mut used = BTreeSet::new();
    let mut variants = Vec::with_capacity(branch_refs.len());

    for branch in branch_refs {
        validate_discriminator_branch_object(
            document,
            &branch.schema_name,
            &discriminator.property_name,
            context,
        )?;
        let tag_value = tag_values
            .get(&branch.schema_name)
            .expect("validated discriminator mappings cover every branch")
            .clone();
        variants.push(ValidatedUnionVariant {
            rust_name: unique_ident(branch.type_name.clone(), &mut used),
            kind: ValidatedUnionVariantKind::Reference {
                type_name: branch.type_name,
                schema_name: branch.schema_name,
            },
            tag_value: Some(tag_value),
        });
    }

    Ok(ValidatedUnion {
        variants,
        tag: Some(ValidatedUnionTag {
            property_name: discriminator.property_name.clone(),
        }),
    })
}

#[derive(Debug)]
struct DiscriminatorBranchRef {
    type_name: String,
    schema_name: String,
}

fn discriminator_union_branches<'a>(
    schema: &'a OasObjectSchema,
    context: &str,
) -> Result<(&'static str, &'a [OasSchema]), ValidationError> {
    match (!schema.any_of.is_empty(), !schema.one_of.is_empty()) {
        (true, false) => Ok(("anyOf", &schema.any_of)),
        (false, true) => Ok(("oneOf", &schema.one_of)),
        (false, false) | (true, true) => Err(ValidationError::InvalidDiscriminatorUnion {
            context: context.to_owned(),
        }),
    }
}

fn validate_discriminator_branch_refs(
    branches: &[OasSchema],
    keyword: &'static str,
    context: &str,
) -> Result<Vec<DiscriminatorBranchRef>, ValidationError> {
    let mut refs = Vec::with_capacity(branches.len());
    let mut used_targets = BTreeSet::new();

    for (index, branch) in branches.iter().enumerate() {
        let Some(reference) = schema_ref(branch, context)? else {
            return Err(ValidationError::UnsupportedDiscriminatorBranch {
                context: context.to_owned(),
                keyword,
                index,
            });
        };
        let schema_name = local_ref_name(reference, "schemas").map_err(|_| {
            ValidationError::UnsupportedDiscriminatorBranch {
                context: context.to_owned(),
                keyword,
                index,
            }
        })?;
        if !used_targets.insert(schema_name.clone()) {
            return Err(ValidationError::InvalidDiscriminatorUnion {
                context: context.to_owned(),
            });
        }
        refs.push(DiscriminatorBranchRef {
            type_name: type_ident(&schema_name),
            schema_name,
        });
    }

    Ok(refs)
}

fn validate_discriminator_mapping(
    discriminator: &OasDiscriminator,
    branches: &[DiscriminatorBranchRef],
    context: &str,
) -> Result<BTreeMap<String, String>, ValidationError> {
    let branch_names = branches
        .iter()
        .map(|branch| branch.schema_name.clone())
        .collect::<BTreeSet<_>>();

    let mut by_schema = branches
        .iter()
        .map(|branch| (branch.schema_name.clone(), branch.schema_name.clone()))
        .collect::<BTreeMap<_, _>>();

    if let Some(mapping) = discriminator.mapping.as_ref() {
        for (value, target) in mapping {
            let Some(schema_name) = discriminator_mapping_schema_name(target, &branch_names) else {
                return Err(ValidationError::InvalidDiscriminatorMapping {
                    context: context.to_owned(),
                    value: value.clone(),
                    target: target.clone(),
                });
            };

            if by_schema.get(&schema_name) != Some(&schema_name) {
                return Err(ValidationError::DuplicateDiscriminatorMapping {
                    context: context.to_owned(),
                    schema: schema_name,
                });
            }

            by_schema.insert(schema_name, value.clone());
        }
    }

    let mut values = BTreeSet::new();
    for branch in branches {
        let value = by_schema
            .get(&branch.schema_name)
            .expect("every branch starts with an implicit discriminator value");
        if !values.insert(value.clone()) {
            return Err(ValidationError::DuplicateDiscriminatorValue {
                context: context.to_owned(),
                value: value.clone(),
            });
        }
    }

    Ok(by_schema)
}

fn discriminator_mapping_schema_name(
    target: &str,
    branch_names: &BTreeSet<String>,
) -> Option<String> {
    let schema_name = if target.starts_with("#/") {
        local_ref_name(target, "schemas").ok()?
    } else if target.contains("://") || target.starts_with('/') {
        return None;
    } else {
        target.to_owned()
    };

    branch_names.contains(&schema_name).then_some(schema_name)
}

fn validate_discriminator_branch_object(
    document: &ResolvedDocument<'_>,
    schema_name: &str,
    property_name: &str,
    context: &str,
) -> Result<(), ValidationError> {
    let fields = discriminator_branch_fields(document, schema_name, context)?;

    if fields.iter().any(|field| field.wire_name == property_name) {
        return Err(ValidationError::DiscriminatorPropertyConflict {
            context: context.to_owned(),
            schema: schema_name.to_owned(),
            property: property_name.to_owned(),
        });
    }

    Ok(())
}

fn discriminator_branch_fields(
    document: &ResolvedDocument<'_>,
    schema_name: &str,
    context: &str,
) -> Result<Vec<ValidatedField>, ValidationError> {
    let schema = component_schema(document, schema_name)?;

    if schema_ref(schema, context)?.is_some() {
        return Err(discriminator_branch_not_object(context, schema_name));
    }

    let schema = object_schema(schema, context)
        .map_err(|_| discriminator_branch_not_object(context, schema_name))?;

    if !schema.all_of.is_empty() {
        let mut stack = vec![];
        return validate_all_of_struct_properties(document, schema_name, schema, &mut stack)
            .map_err(|err| map_discriminator_branch_error(err, context, schema_name));
    }

    let (schema_type, nullable) = schema_type_and_nullable(schema, context)
        .map_err(|_| discriminator_branch_not_object(context, schema_name))?;
    if nullable {
        return Err(discriminator_branch_not_object(context, schema_name));
    }

    match schema_type {
        Some(OasSchemaType::Object) | None if !schema.properties.is_empty() => {
            validate_struct_properties(document, schema_name, schema)
        }
        _ => Err(discriminator_branch_not_object(context, schema_name)),
    }
}

fn map_discriminator_branch_error(
    err: ValidationError,
    context: &str,
    schema_name: &str,
) -> ValidationError {
    match err {
        ValidationError::UnsupportedAllOfBranch { .. }
        | ValidationError::UnsupportedAllOfSiblingKeyword { .. } => {
            discriminator_branch_not_object(context, schema_name)
        }
        other => other,
    }
}

fn discriminator_branch_not_object(context: &str, schema_name: &str) -> ValidationError {
    ValidationError::DiscriminatorBranchNotObject {
        context: context.to_owned(),
        schema: schema_name.to_owned(),
    }
}

fn validate_all_of_struct_properties(
    document: &ResolvedDocument<'_>,
    schema_name: &str,
    schema: &OasObjectSchema,
    stack: &mut Vec<String>,
) -> Result<Vec<ValidatedField>, ValidationError> {
    let context = format!("schema `{schema_name}`");
    reject_all_of_sibling_keywords(schema, &context)?;
    push_all_of_schema(schema_name, stack)?;

    let mut collector = AllOfFieldCollector::new(document, stack);
    let result = collector.collect(schema_name, schema);
    collector.pop_schema();
    result
}

struct AllOfFieldCollector<'a, 'doc> {
    document: &'a ResolvedDocument<'doc>,
    stack: &'a mut Vec<String>,
    fields: Vec<ValidatedField>,
    used: BTreeSet<String>,
}

impl<'a, 'doc> AllOfFieldCollector<'a, 'doc> {
    fn new(document: &'a ResolvedDocument<'doc>, stack: &'a mut Vec<String>) -> Self {
        Self {
            document,
            stack,
            fields: vec![],
            used: BTreeSet::new(),
        }
    }

    fn collect(
        &mut self,
        schema_name: &str,
        schema: &OasObjectSchema,
    ) -> Result<Vec<ValidatedField>, ValidationError> {
        for (index, branch) in schema.all_of.iter().enumerate() {
            self.collect_branch_fields(schema_name, branch, index)?;
        }

        Ok(mem::take(&mut self.fields))
    }

    fn pop_schema(&mut self) {
        self.stack.pop();
    }

    fn collect_branch_fields(
        &mut self,
        schema_name: &str,
        branch: &OasSchema,
        index: usize,
    ) -> Result<(), ValidationError> {
        let context = format!("schema `{schema_name}`");
        if let Some(reference) = schema_ref(branch, &context)? {
            let branch_schema_name = local_ref_name(reference, "schemas").map_err(|_| {
                ValidationError::UnsupportedAllOfBranch {
                    context: context.clone(),
                    index,
                }
            })?;
            return self.collect_component_fields(&branch_schema_name, &context, index);
        }

        let schema = object_schema(branch, &context).map_err(|_| {
            ValidationError::UnsupportedAllOfBranch {
                context: context.clone(),
                index,
            }
        })?;
        self.collect_object_fields(schema_name, schema, &context, index, false)
    }

    fn collect_component_fields(
        &mut self,
        component_schema_name: &str,
        context: &str,
        index: usize,
    ) -> Result<(), ValidationError> {
        push_all_of_schema(component_schema_name, self.stack)?;

        let schema = component_schema(self.document, component_schema_name)?;
        let result =
            self.collect_component_schema_fields(component_schema_name, schema, context, index);

        self.stack.pop();
        result
    }

    fn collect_component_schema_fields(
        &mut self,
        component_schema_name: &str,
        schema: &OasSchema,
        context: &str,
        index: usize,
    ) -> Result<(), ValidationError> {
        if let Some(reference) = schema_ref(schema, context)? {
            let target_schema_name = local_ref_name(reference, "schemas").map_err(|_| {
                ValidationError::UnsupportedAllOfBranch {
                    context: context.to_owned(),
                    index,
                }
            })?;
            return self.collect_component_fields(&target_schema_name, context, index);
        }

        let schema = object_schema(schema, context).map_err(|_| {
            ValidationError::UnsupportedAllOfBranch {
                context: context.to_owned(),
                index,
            }
        })?;
        self.collect_object_fields(component_schema_name, schema, context, index, true)
    }

    fn collect_object_fields(
        &mut self,
        schema_name: &str,
        schema: &OasObjectSchema,
        context: &str,
        index: usize,
        allow_all_of: bool,
    ) -> Result<(), ValidationError> {
        reject_one_of(schema, context)?;
        if !schema.any_of.is_empty() {
            return Err(ValidationError::UnsupportedAllOfBranch {
                context: context.to_owned(),
                index,
            });
        }

        if !schema.all_of.is_empty() {
            if !allow_all_of {
                return Err(ValidationError::UnsupportedAllOfBranch {
                    context: context.to_owned(),
                    index,
                });
            }
            let all_of_context = format!("schema `{schema_name}`");
            reject_all_of_sibling_keywords(schema, &all_of_context)?;
            for (nested_index, branch) in schema.all_of.iter().enumerate() {
                self.collect_branch_fields(schema_name, branch, nested_index)?;
            }
            return Ok(());
        }

        reject_all_of_object_branch_keywords(schema, context, index)?;

        if schema.properties.is_empty() {
            return Err(ValidationError::UnsupportedAllOfBranch {
                context: context.to_owned(),
                index,
            });
        }

        let branch_fields = validate_struct_properties(self.document, schema_name, schema)?;
        self.extend_fields(context, branch_fields)
    }

    fn extend_fields(
        &mut self,
        context: &str,
        branch_fields: Vec<ValidatedField>,
    ) -> Result<(), ValidationError> {
        for field in branch_fields {
            if !self.used.insert(field.wire_name.clone()) {
                return Err(ValidationError::DuplicateAllOfProperty {
                    context: context.to_owned(),
                    property: field.wire_name,
                });
            }
            self.fields.push(field);
        }

        Ok(())
    }
}

fn push_all_of_schema(schema_name: &str, stack: &mut Vec<String>) -> Result<(), ValidationError> {
    if let Some(index) = stack.iter().position(|visited| visited == schema_name) {
        return Err(ValidationError::RecursiveAllOf {
            context: format!("schema `{}`", stack[index]),
            schema: schema_name.to_owned(),
        });
    }

    stack.push(schema_name.to_owned());
    Ok(())
}

fn component_schema<'a>(
    document: &ResolvedDocument<'a>,
    schema_name: &str,
) -> Result<&'a OasSchema, ValidationError> {
    document
        .spec
        .components
        .as_ref()
        .and_then(|components| components.schemas.get(schema_name))
        .ok_or_else(|| ValidationError::MissingJsonPointerToken {
            token: schema_name.to_owned(),
        })
}

fn reject_all_of_sibling_keywords(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    if !all_of_object_type_is_allowed(schema) {
        return Err(ValidationError::UnsupportedAllOfSiblingKeyword {
            context: context.to_owned(),
            keyword: "type".to_owned(),
        });
    }

    for (keyword, present) in [
        ("anyOf", !schema.any_of.is_empty()),
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
            return Err(ValidationError::UnsupportedAllOfSiblingKeyword {
                context: context.to_owned(),
                keyword: keyword.to_owned(),
            });
        }
    }

    if let Some(keyword) = schema.extensions.keys().next() {
        return Err(ValidationError::UnsupportedAllOfSiblingKeyword {
            context: context.to_owned(),
            keyword: format!("x-{keyword}"),
        });
    }

    Ok(())
}

fn reject_all_of_object_branch_keywords(
    schema: &OasObjectSchema,
    context: &str,
    index: usize,
) -> Result<(), ValidationError> {
    if !all_of_object_type_is_allowed(schema) {
        return Err(ValidationError::UnsupportedAllOfBranch {
            context: context.to_owned(),
            index,
        });
    }

    for (keyword, present) in [
        ("enum", !schema.enum_values.is_empty()),
        ("const", schema.const_value.is_some()),
        ("items", schema.items.is_some()),
        ("prefixItems", !schema.prefix_items.is_empty()),
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
        ("format", schema.format.is_some()),
        ("discriminator", schema.discriminator.is_some()),
    ] {
        if present {
            return Err(ValidationError::UnsupportedAllOfSiblingKeyword {
                context: context.to_owned(),
                keyword: keyword.to_owned(),
            });
        }
    }

    if let Some(keyword) = schema.extensions.keys().next() {
        return Err(ValidationError::UnsupportedAllOfSiblingKeyword {
            context: context.to_owned(),
            keyword: format!("x-{keyword}"),
        });
    }

    Ok(())
}

fn all_of_object_type_is_allowed(schema: &OasObjectSchema) -> bool {
    matches!(
        schema.schema_type.as_ref(),
        None | Some(OasSchemaTypeSet::Single(OasSchemaType::Object))
    )
}

fn reject_any_of_sibling_keywords(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    reject_plain_union_sibling_keywords(schema, context, PlainUnionKeyword::AnyOf)
}

fn reject_plain_one_of_sibling_keywords(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    reject_plain_union_sibling_keywords(schema, context, PlainUnionKeyword::OneOf)
}

#[derive(Clone, Copy)]
enum PlainUnionKeyword {
    AnyOf,
    OneOf,
}

impl PlainUnionKeyword {
    fn error(self, context: String, keyword: String) -> ValidationError {
        match self {
            Self::AnyOf => ValidationError::UnsupportedAnyOfSiblingKeyword { context, keyword },
            Self::OneOf => ValidationError::UnsupportedOneOfSiblingKeyword { context, keyword },
        }
    }

    fn branch_error(self, context: &str, index: usize) -> ValidationError {
        match self {
            Self::AnyOf => ValidationError::UnsupportedAnyOfBranch {
                context: context.to_owned(),
                index,
            },
            Self::OneOf => ValidationError::UnsupportedOneOfBranch {
                context: context.to_owned(),
                index,
            },
        }
    }
}

fn reject_plain_union_sibling_keywords(
    schema: &OasObjectSchema,
    context: &str,
    union_keyword: PlainUnionKeyword,
) -> Result<(), ValidationError> {
    for (keyword, present) in [
        (
            "anyOf",
            matches!(union_keyword, PlainUnionKeyword::OneOf) && !schema.any_of.is_empty(),
        ),
        (
            "oneOf",
            matches!(union_keyword, PlainUnionKeyword::AnyOf) && !schema.one_of.is_empty(),
        ),
        ("allOf", !schema.all_of.is_empty()),
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
            return Err(union_keyword.error(context.to_owned(), keyword.to_owned()));
        }
    }

    if let Some(keyword) = unsupported_union_extension(schema) {
        return Err(union_keyword.error(context.to_owned(), keyword));
    }

    Ok(())
}

fn reject_discriminator_union_sibling_keywords(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    for (keyword, present) in [
        ("allOf", !schema.all_of.is_empty()),
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
    ] {
        if present {
            return Err(ValidationError::UnsupportedAnyOfSiblingKeyword {
                context: context.to_owned(),
                keyword: keyword.to_owned(),
            });
        }
    }

    if let Some(keyword) = unsupported_union_extension(schema) {
        return Err(ValidationError::UnsupportedAnyOfSiblingKeyword {
            context: context.to_owned(),
            keyword,
        });
    }

    Ok(())
}

fn unsupported_union_extension(schema: &OasObjectSchema) -> Option<String> {
    schema
        .extensions
        .keys()
        .find(|keyword| keyword.as_str() == "satay" || keyword.as_str() == "x-satay")
        .map(|keyword| extension_wire_keyword(keyword))
}

fn extension_wire_keyword(keyword: &str) -> String {
    if keyword.starts_with("x-") {
        keyword.to_owned()
    } else {
        format!("x-{keyword}")
    }
}

fn reject_any_of_cycles(components: &[ValidatedComponent]) -> Result<(), ValidationError> {
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
        ValidatedComponentKind::Struct(fields) => {
            fields.iter().any(|field| field.ty.contains_any_of())
        }
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
                collect_type_union_targets(&field.ty, schemas_by_rust_name, targets);
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
            targets.extend(
                union
                    .variants
                    .iter()
                    .filter_map(|variant| match &variant.kind {
                        ValidatedUnionVariantKind::Reference { schema_name, .. } => {
                            Some(schema_name.clone())
                        }
                        ValidatedUnionVariantKind::Inline(_) => None,
                    }),
            );
        }
        ValidatedTypeKind::Array(item) => {
            collect_type_union_targets(item, schemas_by_rust_name, targets);
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
            kind: ValidatedTypeKind::Enum(validated_enum(
                schema,
                &explicit_variants,
                EnumFallback::None,
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

fn validated_enum(
    schema: &OasObjectSchema,
    explicit_variants: &BTreeMap<String, String>,
    fallback: EnumFallback,
    context: &str,
) -> Result<Enum, ValidationError> {
    let mut used = BTreeSet::from(["Other".to_owned()]);

    for rust_name in explicit_variants.values() {
        used.insert(rust_name.clone());
    }

    let mut variants = Vec::with_capacity(schema.enum_values.len());

    for value in &schema.enum_values {
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
