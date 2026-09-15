//! Owned schema conversion, with references kept behind source-name identities.

use core::slice;
use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use oas3::spec::{
    BooleanSchema as OasBooleanSchema, Discriminator as OasDiscriminator,
    ObjectSchema as OasObjectSchema, Schema as OasSchema, SchemaType as OasSchemaType,
    SchemaTypeSet as OasSchemaTypeSet,
};
use satay_ir::{
    AdditionalProperties, ArraySchema, CompositionKind, CompositionSchema, Definition,
    DefinitionId, Discriminator, DiscriminatorMapping, IntegerSchema, NumberSchema, ObjectSchema,
    Property, PropertyPolicy, SchemaAnnotations, SchemaUse, StringInterpretation, StringSchema,
    TypeExpr,
};
use serde_json::Value as JsonValue;

use super::constraint::{array_constraints, numeric_constraints, string_constraints};
use super::interpretation::InterpretedUse;
use super::source::{child_pointer, source_ref};
use super::{NormalizeContext, NormalizeError, ValidationErrorExt};
use crate::error::ValidationError;
use crate::parse::helpers::optional_description;
use crate::parse::reference::{schema_component_ref, schema_type_and_nullable};
use crate::parse::satay::schema_options;
use crate::parse::validate::constraint::reject_keyword;
use crate::parse::validate::schema::{
    annotation_only_all_of_ref_wrapper, reject_all_of_object_branch_keywords,
    reject_all_of_sibling_keywords, reject_any_of_sibling_keywords,
    reject_discriminator_union_sibling_keywords, reject_plain_one_of_sibling_keywords,
    reject_preserved_unknown_keywords, unsupported_reference_schema_keyword, validate_enum_shape,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SchemaPosition {
    Definition,
    Property,
    Value,
    UnionBranch,
    AllOfBranch,
    RetainedWire,
}

impl NormalizeContext<'_, '_> {
    pub(super) fn define_all(
        &self,
        builder: &mut satay_ir::ApiBuilder,
    ) -> Result<(), NormalizeError> {
        let Some(components) = self.document.spec.components.as_ref() else {
            return Ok(());
        };
        for (name, schema) in &components.schemas {
            if self.excluded.contains(name) {
                continue;
            }
            let pointer = child_pointer("/components/schemas", name);
            let id = self.schema_definition_id(name, &pointer)?;
            let value = self.schema_use(
                schema,
                &pointer,
                SchemaPosition::Definition,
                &format!("schema `{name}`"),
            )?;
            builder.define(
                id,
                Definition {
                    source_name: name.clone(),
                    schema: value,
                },
            )?;
        }
        Ok(())
    }

    /// Checks unknown vocabulary once at the retained root, not at every child.
    pub(super) fn schema_use(
        &self,
        schema: &OasSchema,
        pointer: &str,
        position: SchemaPosition,
        context: &str,
    ) -> Result<SchemaUse, NormalizeError> {
        reject_preserved_unknown_keywords(schema, context).map_err(|error| {
            let location =
                unknown_keyword_pointer(schema, pointer).unwrap_or_else(|| pointer.to_owned());
            error.at(self, &location)
        })?;
        self.schema_use_inner(schema, pointer, position, false, context)
            .map(|(value, _)| value)
    }

    /// The wire permission is separate from position: properties inside retained
    /// envelopes still own property-local options, and ignored fields retain their
    /// complete wire shape without becoming generated inline objects.
    fn schema_use_inner(
        &self,
        schema: &OasSchema,
        pointer: &str,
        position: SchemaPosition,
        retained_wire: bool,
        context: &str,
    ) -> Result<(SchemaUse, PropertyPolicy), NormalizeError> {
        let schema = schema.as_object().ok_or_else(|| {
            ValidationError::UnsupportedBooleanSchema {
                context: context.to_owned(),
            }
            .at(self, pointer)
        })?;
        let is_null = is_null_branch(schema) && position == SchemaPosition::UnionBranch;
        let (schema_type, nullable) = if schema.reference.is_some() {
            (None, false)
        } else if is_null {
            (Some(OasSchemaType::Null), false)
        } else {
            schema_type_and_nullable(schema, context).map_err(|error| error.at(self, pointer))?
        };
        // The typed parser erases explicit null defaults. Recover presence
        // before extension validation so forbidden ref siblings keep precedence.
        if schema.reference.is_some()
            && unsupported_reference_schema_keyword(schema).is_none()
            && self.presence.has(pointer, "default")
        {
            return Err(ValidationError::UnsupportedRefSiblingKeyword {
                context: context.to_owned(),
                keyword: "default".to_owned(),
            }
            .at(self, &child_pointer(pointer, "default")));
        }
        let mut interpreted =
            self.use_options_impl(schema, schema_type, position, pointer, context)?;
        let retained_wire = retained_wire
            || position == SchemaPosition::RetainedWire
            || interpreted.policy == PropertyPolicy::Ignored;
        let annotations = SchemaAnnotations {
            description: optional_description(&schema.description),
            format: schema.format.clone(),
            default: schema.default.clone().or_else(|| {
                self.presence
                    .has(pointer, "default")
                    .then_some(JsonValue::Null)
            }),
            source: Some(source_ref(self.document_id, pointer)),
        };
        let ty = if let Some(reference) = schema.reference.as_deref() {
            let reference =
                schema_component_ref(reference).map_err(|error| error.at(self, pointer))?;
            TypeExpr::Ref(self.schema_definition_id(reference.name(), pointer)?)
        } else if schema.discriminator.is_some()
            || !schema.any_of.is_empty()
            || !schema.one_of.is_empty()
        {
            self.union_schema(schema, pointer, retained_wire, context)?
        } else if !schema.all_of.is_empty() {
            self.all_of_schema(schema, pointer, retained_wire, context)?
        } else {
            self.plain_schema(
                schema,
                schema_type,
                pointer,
                position,
                retained_wire,
                context,
                &mut interpreted,
            )?
        };
        Ok((
            SchemaUse {
                ty,
                nullable,
                annotations,
            },
            interpreted.policy,
        ))
    }

    fn schema_definition_id(
        &self,
        name: &str,
        pointer: &str,
    ) -> Result<DefinitionId, NormalizeError> {
        match self.definitions.get(name) {
            Some(&id) => Ok(id),
            None if self.excluded.contains(name) => Err(NormalizeError::ExcludedDefinition {
                name: name.to_owned(),
                location: source_ref(self.document_id, pointer),
            }),
            None => Err(ValidationError::MissingJsonPointerToken {
                token: name.to_owned(),
            }
            .at(self, pointer)),
        }
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn plain_schema(
        &self,
        schema: &OasObjectSchema,
        schema_type: Option<OasSchemaType>,
        pointer: &str,
        position: SchemaPosition,
        retained_wire: bool,
        context: &str,
        interpreted: &mut InterpretedUse,
    ) -> Result<TypeExpr, NormalizeError> {
        if !schema.enum_values.is_empty() {
            validate_enum_shape(&schema.enum_values, schema_type, context)
                .map_err(|error| error.at(self, &child_pointer(pointer, "enum")))?;
        }

        if let Some(value) = &schema.const_value {
            validate_enum_shape(slice::from_ref(value), schema_type, context)
                .map_err(|error| error.at(self, &child_pointer(pointer, "const")))?;
            if !schema.enum_values.is_empty() && !schema.enum_values.contains(value) {
                return Err(ValidationError::ConstNotInEnum {
                    context: context.to_owned(),
                }
                .at(self, &child_pointer(pointer, "const")));
            }
        }

        let schema_type = schema_type.or_else(|| {
            if !schema.enum_values.is_empty() || schema.const_value.is_some() {
                Some(OasSchemaType::String)
            } else if !schema.properties.is_empty() || schema.additional_properties.is_some() {
                Some(OasSchemaType::Object)
            } else {
                None
            }
        });

        self.reject_unrepresented_keywords(schema, schema_type, interpreted, pointer, context)?;

        match schema_type {
            Some(OasSchemaType::String) => {
                let enum_values = if schema.enum_values.is_empty() {
                    None
                } else {
                    Some(
                        schema
                            .enum_values
                            .iter()
                            .map(|value| {
                                value.as_str().map(str::to_owned).ok_or_else(|| {
                                    ValidationError::NonStringEnumValue {
                                        context: context.to_owned(),
                                    }
                                    .at(self, &child_pointer(pointer, "enum"))
                                })
                            })
                            .collect::<Result<_, _>>()?,
                    )
                };
                Ok(TypeExpr::String(StringSchema {
                    constraints: string_constraints(schema, context)
                        .map_err(|error| error.at(self, pointer))?,
                    enum_values,
                    const_value: schema
                        .const_value
                        .as_ref()
                        .and_then(JsonValue::as_str)
                        .map(str::to_owned),
                    enum_variants: mem::take(&mut interpreted.enum_variants),
                    interpretation: mem::take(&mut interpreted.string),
                }))
            }
            Some(OasSchemaType::Integer) => Ok(TypeExpr::Integer(IntegerSchema {
                constraints: numeric_constraints(schema, true, context)
                    .map_err(|error| error.at(self, pointer))?,
                interpretation: mem::take(&mut interpreted.integer),
            })),
            Some(OasSchemaType::Number) => Ok(TypeExpr::Number(NumberSchema {
                constraints: numeric_constraints(schema, false, context)
                    .map_err(|error| error.at(self, pointer))?,
            })),
            Some(OasSchemaType::Boolean) => Ok(TypeExpr::Boolean),
            Some(OasSchemaType::Null) => Ok(TypeExpr::Null),
            Some(OasSchemaType::Array) => {
                let items = schema.items.as_deref().ok_or_else(|| {
                    ValidationError::MissingArrayItems {
                        context: context.to_owned(),
                    }
                    .at(self, pointer)
                })?;
                let (items, _) = self.schema_use_inner(
                    items,
                    &child_pointer(pointer, "items"),
                    SchemaPosition::Value,
                    retained_wire,
                    &format!("{context} items"),
                )?;
                Ok(TypeExpr::Array(ArraySchema {
                    items: Box::new(items),
                    constraints: array_constraints(schema, context)
                        .map_err(|error| error.at(self, pointer))?,
                }))
            }
            Some(OasSchemaType::Object) => {
                if !schema.properties.is_empty()
                    && !retained_wire
                    && !matches!(
                        position,
                        SchemaPosition::Definition | SchemaPosition::AllOfBranch
                    )
                {
                    return Err(ValidationError::InlineObjectSchema {
                        context: context.to_owned(),
                    }
                    .at(self, pointer));
                }

                let properties_pointer = child_pointer(pointer, "properties");
                let mut properties = Vec::with_capacity(schema.properties.len());

                for (name, property_schema) in &schema.properties {
                    let (value, policy) = self.schema_use_inner(
                        property_schema,
                        &child_pointer(&properties_pointer, name),
                        SchemaPosition::Property,
                        retained_wire,
                        &format!("{context}.{name}"),
                    )?;
                    properties.push(Property {
                        wire_name: name.clone(),
                        required: schema.required.contains(name),
                        value,
                        policy,
                    });
                }

                let additional_properties = match schema.additional_properties.as_ref() {
                    None => AdditionalProperties::Unspecified,
                    Some(OasSchema::Boolean(OasBooleanSchema(true))) => {
                        AdditionalProperties::Allowed
                    }
                    Some(OasSchema::Boolean(OasBooleanSchema(false))) => {
                        AdditionalProperties::Forbidden
                    }
                    Some(value @ OasSchema::Object(_)) => {
                        let (value, _) = self.schema_use_inner(
                            value,
                            &child_pointer(pointer, "additionalProperties"),
                            SchemaPosition::RetainedWire,
                            true,
                            &format!("{context} additionalProperties"),
                        )?;
                        AdditionalProperties::Schema(Box::new(value))
                    }
                };
                Ok(TypeExpr::Object(ObjectSchema {
                    properties,
                    additional_properties,
                }))
            }
            None => Ok(TypeExpr::AnyJson),
        }
    }

    fn reject_unrepresented_keywords(
        &self,
        schema: &OasObjectSchema,
        schema_type: Option<OasSchemaType>,
        interpreted: &InterpretedUse,
        pointer: &str,
        context: &str,
    ) -> Result<(), NormalizeError> {
        let numeric = matches!(
            schema_type,
            Some(OasSchemaType::Integer | OasSchemaType::Number)
        ) || matches!(
            interpreted.string,
            StringInterpretation::IntegerRange { .. } | StringInterpretation::NumberRange { .. }
        );
        let string = schema_type == Some(OasSchemaType::String);
        let array = schema_type == Some(OasSchemaType::Array);
        let object = schema_type == Some(OasSchemaType::Object);
        for (keyword, unsupported) in [
            ("prefixItems", !schema.prefix_items.is_empty()),
            ("multipleOf", schema.multiple_of.is_some()),
            ("minProperties", schema.min_properties.is_some()),
            ("maxProperties", schema.max_properties.is_some()),
            ("minimum", !numeric && schema.minimum.is_some()),
            ("maximum", !numeric && schema.maximum.is_some()),
            (
                "exclusiveMinimum",
                !numeric && schema.exclusive_minimum.is_some(),
            ),
            (
                "exclusiveMaximum",
                !numeric && schema.exclusive_maximum.is_some(),
            ),
            ("minLength", !string && schema.min_length.is_some()),
            ("maxLength", !string && schema.max_length.is_some()),
            ("pattern", !string && schema.pattern.is_some()),
            ("items", !array && schema.items.is_some()),
            ("minItems", !array && schema.min_items.is_some()),
            ("maxItems", !array && schema.max_items.is_some()),
            ("properties", !object && !schema.properties.is_empty()),
            (
                "additionalProperties",
                !object && schema.additional_properties.is_some(),
            ),
            ("required", !object && !schema.required.is_empty()),
        ] {
            reject_keyword(unsupported, keyword, context)
                .map_err(|error| error.at(self, &child_pointer(pointer, keyword)))?;
        }

        if schema.unique_items == Some(true) {
            return Err(ValidationError::UniqueItemsUnsupported {
                context: context.to_owned(),
            }
            .at(self, &child_pointer(pointer, "uniqueItems")));
        }
        Ok(())
    }

    fn all_of_schema(
        &self,
        schema: &OasObjectSchema,
        pointer: &str,
        retained_wire: bool,
        context: &str,
    ) -> Result<TypeExpr, NormalizeError> {
        self.all_of_fields(schema, pointer, context, &mut BTreeSet::new())?;
        let mut branches = Vec::with_capacity(schema.all_of.len());

        for (index, branch) in schema.all_of.iter().enumerate() {
            let branch_pointer =
                child_pointer(&child_pointer(pointer, "allOf"), &index.to_string());
            let (value, _) = self.schema_use_inner(
                branch,
                &branch_pointer,
                SchemaPosition::AllOfBranch,
                retained_wire,
                &format!("{context}.allOf[{index}]"),
            )?;
            branches.push(value);
        }

        Ok(TypeExpr::Composition(CompositionSchema {
            kind: CompositionKind::AllOf,
            branches,
            discriminator: None,
        }))
    }

    fn union_schema(
        &self,
        schema: &OasObjectSchema,
        pointer: &str,
        retained_wire: bool,
        context: &str,
    ) -> Result<TypeExpr, NormalizeError> {
        let (kind, keyword, declared) = if schema.discriminator.is_some() {
            reject_discriminator_union_sibling_keywords(schema, context)
                .map_err(|error| error.at(self, pointer))?;

            match (!schema.any_of.is_empty(), !schema.one_of.is_empty()) {
                (true, false) => (CompositionKind::AnyOf, "anyOf", &schema.any_of),
                (false, true) => (CompositionKind::OneOf, "oneOf", &schema.one_of),
                _ => {
                    return Err(ValidationError::InvalidDiscriminatorUnion {
                        context: context.to_owned(),
                    }
                    .at(self, &child_pointer(pointer, "discriminator")));
                }
            }
        } else if !schema.any_of.is_empty() {
            reject_any_of_sibling_keywords(schema, context)
                .map_err(|error| error.at(self, pointer))?;
            (CompositionKind::AnyOf, "anyOf", &schema.any_of)
        } else {
            reject_plain_one_of_sibling_keywords(schema, context)
                .map_err(|error| error.at(self, pointer))?;
            (CompositionKind::OneOf, "oneOf", &schema.one_of)
        };

        let mut null_seen = false;
        let mut non_null = false;
        let mut branches = Vec::with_capacity(declared.len());

        for (index, branch) in declared.iter().enumerate() {
            let branch_pointer =
                child_pointer(&child_pointer(pointer, keyword), &index.to_string());
            if schema.discriminator.is_none() {
                self.check_union_branch(branch, keyword, index, &branch_pointer, context)?;
                if branch.as_object().is_some_and(is_null_branch) {
                    if null_seen {
                        return Err(ValidationError::DuplicateUnionNullBranch {
                            context: context.to_owned(),
                            keyword,
                            index,
                        }
                        .at(self, &branch_pointer));
                    }
                    null_seen = true;
                } else {
                    non_null = true;
                }
            }

            let (value, _) = self.schema_use_inner(
                branch,
                &branch_pointer,
                SchemaPosition::UnionBranch,
                retained_wire,
                &format!("{context}.{keyword}[{index}]"),
            )?;
            branches.push(value);
        }

        if schema.discriminator.is_none() && !non_null {
            return Err(ValidationError::NullableUnionWithoutVariants {
                context: context.to_owned(),
                keyword,
            }
            .at(self, pointer));
        }

        let discriminator = schema
            .discriminator
            .as_ref()
            .map(|discriminator| {
                self.discriminator(discriminator, declared, keyword, pointer, context)
            })
            .transpose()?;

        Ok(TypeExpr::Composition(CompositionSchema {
            kind,
            branches,
            discriminator,
        }))
    }

    fn check_union_branch(
        &self,
        branch: &OasSchema,
        keyword: &'static str,
        index: usize,
        pointer: &str,
        context: &str,
    ) -> Result<(), NormalizeError> {
        let unsupported = || {
            if keyword == "anyOf" {
                ValidationError::UnsupportedAnyOfBranch {
                    context: context.to_owned(),
                    index,
                }
            } else {
                ValidationError::UnsupportedOneOfBranch {
                    context: context.to_owned(),
                    index,
                }
            }
            .at(self, pointer)
        };

        let schema = branch.as_object().ok_or_else(unsupported)?;

        if schema.reference.is_some()
            || is_null_branch(schema)
            || annotation_only_all_of_ref_wrapper(schema).is_some()
        {
            return Ok(());
        }

        if schema.discriminator.is_some() && !schema.one_of.is_empty() && schema.any_of.is_empty() {
            return Ok(());
        }

        if !schema.any_of.is_empty() || !schema.one_of.is_empty() || !schema.all_of.is_empty() {
            return Err(unsupported());
        }

        let (kind, nullable) =
            schema_type_and_nullable(schema, context).map_err(|_| unsupported())?;

        if nullable {
            return Err(unsupported());
        }

        if schema.const_value.is_some() || !schema.enum_values.is_empty() {
            if !schema.enum_values.is_empty() && kind != Some(OasSchemaType::String) {
                return Err(unsupported());
            }
            return Ok(());
        }

        match kind {
            Some(
                OasSchemaType::String
                | OasSchemaType::Integer
                | OasSchemaType::Number
                | OasSchemaType::Boolean
                | OasSchemaType::Array,
            ) => Ok(()),
            Some(OasSchemaType::Object)
                if schema.properties.is_empty()
                    && matches!(
                        schema.additional_properties.as_ref(),
                        Some(OasSchema::Object(_) | OasSchema::Boolean(OasBooleanSchema(true)))
                    ) =>
            {
                Ok(())
            }
            _ => Err(unsupported()),
        }
    }

    /// Follows references only for the concrete field-set query. This does not
    /// expand references in the emitted graph or select a Rust layout.
    fn object_fields<'s>(
        &'s self,
        schema: &'s OasSchema,
        pointer: &str,
        context: &str,
        active: &mut BTreeSet<String>,
        all_of_branch: Option<AllOfBranch>,
    ) -> Result<Option<Vec<DeclaredProperty<'s>>>, NormalizeError> {
        let Some(object) = schema.as_object() else {
            return Ok(None);
        };

        if let Some(reference) = object.reference.as_deref() {
            let reference =
                schema_component_ref(reference).map_err(|error| error.at(self, pointer))?;
            let name = reference.name();
            self.schema_definition_id(name, pointer)?;
            if !active.insert(name.to_owned()) {
                return Err(ValidationError::RecursiveAllOf {
                    context: context.to_owned(),
                    schema: name.to_owned(),
                }
                .at(self, pointer));
            }
            let target = self.component_schema(name, pointer)?;
            let result = self.object_fields(
                target,
                &child_pointer("/components/schemas", name),
                context,
                active,
                all_of_branch.map(|branch| AllOfBranch {
                    inline: false,
                    ..branch
                }),
            );
            active.remove(name);
            return result;
        }

        if !object.all_of.is_empty() {
            if let Some(AllOfBranch {
                index,
                inline: true,
            }) = all_of_branch
            {
                return Err(ValidationError::UnsupportedAllOfBranch {
                    context: context.to_owned(),
                    index,
                }
                .at(self, pointer));
            }
            return self.all_of_fields(object, pointer, context, active);
        }

        if !object.any_of.is_empty() || !object.one_of.is_empty() {
            return Ok(None);
        }

        let (kind, nullable) =
            schema_type_and_nullable(object, context).map_err(|error| error.at(self, pointer))?;

        if nullable
            || !matches!(kind, Some(OasSchemaType::Object) | None)
            || object.properties.is_empty()
        {
            return Ok(None);
        }

        if let Some(AllOfBranch { index, .. }) = all_of_branch {
            reject_all_of_object_branch_keywords(object, context, index)
                .map_err(|error| error.at(self, pointer))?;
        }

        let mut fields = Vec::with_capacity(object.properties.len());

        for (name, schema) in &object.properties {
            let pointer = child_pointer(&child_pointer(pointer, "properties"), name);
            self.check_nested_all_of(schema, &pointer, context, active)?;
            fields.push(DeclaredProperty {
                name,
                schema,
                required: object.required.contains(name),
                pointer,
            });
        }
        Ok(Some(fields))
    }

    fn all_of_fields<'s>(
        &'s self,
        schema: &'s OasObjectSchema,
        pointer: &str,
        context: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Option<Vec<DeclaredProperty<'s>>>, NormalizeError> {
        let wrapper = annotation_only_all_of_ref_wrapper(schema).is_some();
        if !wrapper {
            reject_all_of_sibling_keywords(schema, context)
                .map_err(|error| error.at(self, pointer))?;
        }

        let mut fields = vec![];
        let mut names = BTreeSet::new();

        for (index, branch) in schema.all_of.iter().enumerate() {
            let branch_pointer =
                child_pointer(&child_pointer(pointer, "allOf"), &index.to_string());

            let branch_fields = self.object_fields(
                branch,
                &branch_pointer,
                context,
                active,
                Some(AllOfBranch {
                    index,
                    inline: true,
                }),
            )?;

            let Some(branch_fields) = branch_fields else {
                if wrapper {
                    return Ok(None);
                }
                return Err(ValidationError::UnsupportedAllOfBranch {
                    context: context.to_owned(),
                    index,
                }
                .at(self, &branch_pointer));
            };

            for field in branch_fields {
                if !names.insert(field.name) {
                    return Err(ValidationError::DuplicateAllOfProperty {
                        context: context.to_owned(),
                        property: field.name.to_owned(),
                    }
                    .at(self, &field.pointer));
                }
                fields.push(field);
            }
        }

        Ok(Some(fields))
    }

    fn check_nested_all_of(
        &self,
        schema: &OasSchema,
        pointer: &str,
        context: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<(), NormalizeError> {
        let Some(object) = schema.as_object() else {
            return Ok(());
        };

        if object.reference.is_some() {
            return Ok(());
        }

        if !object.all_of.is_empty() {
            self.all_of_fields(object, pointer, context, active)?;
            return Ok(());
        }

        for (name, child) in &object.properties {
            self.check_nested_all_of(
                child,
                &child_pointer(&child_pointer(pointer, "properties"), name),
                context,
                active,
            )?;
        }

        if let Some(items) = object.items.as_deref() {
            self.check_nested_all_of(items, &child_pointer(pointer, "items"), context, active)?;
        }

        if let Some(value) = &object.additional_properties {
            self.check_nested_all_of(
                value,
                &child_pointer(pointer, "additionalProperties"),
                context,
                active,
            )?;
        }

        for (keyword, branches) in [("anyOf", &object.any_of), ("oneOf", &object.one_of)] {
            for (index, branch) in branches.iter().enumerate() {
                self.check_nested_all_of(
                    branch,
                    &child_pointer(&child_pointer(pointer, keyword), &index.to_string()),
                    context,
                    active,
                )?;
            }
        }
        Ok(())
    }

    fn component_schema(&self, name: &str, pointer: &str) -> Result<&OasSchema, NormalizeError> {
        self.document
            .spec
            .components
            .as_ref()
            .and_then(|components| components.schemas.get(name))
            .ok_or_else(|| {
                ValidationError::MissingJsonPointerToken {
                    token: name.to_owned(),
                }
                .at(self, pointer)
            })
    }

    #[allow(clippy::too_many_lines)]
    fn discriminator(
        &self,
        discriminator: &OasDiscriminator,
        branches: &[OasSchema],
        keyword: &'static str,
        pointer: &str,
        context: &str,
    ) -> Result<Discriminator, NormalizeError> {
        let location = child_pointer(pointer, "discriminator");
        let mut targets = BTreeMap::new();

        for (index, branch) in branches.iter().enumerate() {
            let branch_pointer =
                child_pointer(&child_pointer(pointer, keyword), &index.to_string());

            let reference = branch.reference().ok_or_else(|| {
                ValidationError::UnsupportedDiscriminatorBranch {
                    context: context.to_owned(),
                    keyword,
                    index,
                }
                .at(self, &branch_pointer)
            })?;

            let reference =
                schema_component_ref(reference).map_err(|error| error.at(self, &branch_pointer))?;

            let name = reference.name().to_owned();

            if targets.contains_key(&name) {
                return Err(ValidationError::InvalidDiscriminatorUnion {
                    context: context.to_owned(),
                }
                .at(self, &branch_pointer));
            }

            let fields = self
                .object_fields(branch, &branch_pointer, context, &mut BTreeSet::new(), None)?
                .ok_or_else(|| {
                    ValidationError::DiscriminatorBranchNotObject {
                        context: context.to_owned(),
                        schema: name.clone(),
                    }
                    .at(self, &branch_pointer)
                })?;

            let tag = if let Some(field) = fields
                .iter()
                .find(|field| field.name == discriminator.property_name)
            {
                let invalid = || {
                    ValidationError::InvalidDiscriminatorProperty {
                    context: context.to_owned(), schema: name.clone(),
                    property: discriminator.property_name.clone(),
                    expected: "a strict, required, non-null singleton string enum or string const",
                }.at(self, &field.pointer)
                };

                let options = field
                    .schema
                    .as_object()
                    .map(|schema| schema_options(schema, context))
                    .transpose()
                    .map_err(|error| error.at(self, &field.pointer))?
                    .flatten();

                if !field.required
                    || options.as_ref().is_some_and(|options| {
                        options.ignore == Some(true)
                            || options.treat_error_as_none == Some(true)
                            || options.none_if.is_some()
                    })
                {
                    return Err(invalid());
                }

                Some(
                    self.singleton_tag(
                        field.schema,
                        &field.pointer,
                        context,
                        &mut BTreeSet::new(),
                    )?
                    .ok_or_else(invalid)?,
                )
            } else {
                None
            };
            targets.insert(name, tag);
        }

        let has_tags = targets.values().any(Option::is_some);
        if has_tags && let Some((name, _)) = targets.iter().find(|(_, tag)| tag.is_none()) {
            return Err(ValidationError::InvalidDiscriminatorProperty {
                context: context.to_owned(),
                schema: name.clone(),
                property: discriminator.property_name.clone(),
                expected: "present on every branch when any branch contains it",
            }
            .at(self, &location));
        }

        let mut explicit = BTreeMap::new();
        let mut mappings = vec![];

        if let Some(declarations) = &discriminator.mapping {
            for (value, target) in declarations {
                let mapping_pointer = child_pointer(&child_pointer(&location, "mapping"), value);
                let name = if target.starts_with("#/") {
                    schema_component_ref(target)
                        .ok()
                        .map(|reference| reference.name().to_owned())
                } else if target.starts_with('/') || target.contains("://") {
                    None
                } else {
                    Some(target.clone())
                };
                let name = name
                    .filter(|name| targets.contains_key(name))
                    .ok_or_else(|| {
                        ValidationError::InvalidDiscriminatorMapping {
                            context: context.to_owned(),
                            value: value.clone(),
                            target: target.clone(),
                        }
                        .at(self, &mapping_pointer)
                    })?;
                if explicit.insert(name.clone(), value.clone()).is_some() {
                    return Err(ValidationError::DuplicateDiscriminatorMapping {
                        context: context.to_owned(),
                        schema: name,
                    }
                    .at(self, &mapping_pointer));
                }
                if let Some(Some(actual)) = targets.get(&name)
                    && actual != value
                {
                    return Err(ValidationError::DiscriminatorMappingValueMismatch {
                        context: context.to_owned(),
                        schema: name,
                        value: value.clone(),
                        actual: actual.clone(),
                    }
                    .at(self, &mapping_pointer));
                }
                mappings.push(DiscriminatorMapping {
                    wire_value: value.clone(),
                    target: self.schema_definition_id(&name, &mapping_pointer)?,
                    source: Some(source_ref(self.document_id, &mapping_pointer)),
                });
            }
        }

        let mut values = BTreeSet::new();

        for (name, tag) in &targets {
            let value = tag.as_ref().or_else(|| explicit.get(name)).unwrap_or(name);
            if !values.insert(value) {
                return Err(ValidationError::DuplicateDiscriminatorValue {
                    context: context.to_owned(),
                    value: value.clone(),
                }
                .at(self, &location));
            }
        }

        Ok(Discriminator {
            property_name: discriminator.property_name.clone(),
            mappings,
        })
    }

    fn singleton_tag(
        &self,
        schema: &OasSchema,
        pointer: &str,
        context: &str,
        active: &mut BTreeSet<String>,
    ) -> Result<Option<String>, NormalizeError> {
        let Some(schema) = schema.as_object() else {
            return Ok(None);
        };

        let (kind, nullable) =
            schema_type_and_nullable(schema, context).map_err(|error| error.at(self, pointer))?;

        if nullable {
            return Ok(None);
        }

        if let Some(reference) = schema
            .reference
            .as_deref()
            .or_else(|| annotation_only_all_of_ref_wrapper(schema))
        {
            let reference =
                schema_component_ref(reference).map_err(|error| error.at(self, pointer))?;
            let name = reference.name();
            if !active.insert(name.to_owned()) {
                return Ok(None);
            }
            let value = self.singleton_tag(
                self.component_schema(name, pointer)?,
                &child_pointer("/components/schemas", name),
                context,
                active,
            );
            active.remove(name);
            return value;
        }

        if !matches!(kind, None | Some(OasSchemaType::String))
            || !schema.any_of.is_empty()
            || !schema.one_of.is_empty()
            || !schema.all_of.is_empty()
        {
            return Ok(None);
        }

        if let Some(value) = &schema.const_value {
            if !schema.enum_values.is_empty() && !schema.enum_values.contains(value) {
                return Err(ValidationError::ConstNotInEnum {
                    context: context.to_owned(),
                }
                .at(self, &child_pointer(pointer, "const")));
            }
            return Ok(value.as_str().map(str::to_owned));
        }

        match schema.enum_values.as_slice() {
            [value] => Ok(value.as_str().map(str::to_owned)),
            _ => Ok(None),
        }
    }
}

#[derive(Clone, Copy)]
struct AllOfBranch {
    index: usize,
    inline: bool,
}

struct DeclaredProperty<'a> {
    name: &'a str,
    schema: &'a OasSchema,
    required: bool,
    pointer: String,
}

fn is_null_branch(schema: &OasObjectSchema) -> bool {
    matches!(
        schema.schema_type.as_ref(),
        Some(OasSchemaTypeSet::Single(OasSchemaType::Null))
    )
}

/// Locate an unknown keyword only after the shared recursive guard has failed.
fn unknown_keyword_pointer(schema: &OasSchema, pointer: &str) -> Option<String> {
    let schema = schema.as_object()?;

    if let Some(keyword) = schema
        .present_keywords()
        .find(|keyword| schema.unknown_keywords.contains_key(*keyword))
    {
        return Some(child_pointer(pointer, keyword));
    }

    for (keyword, branches) in [
        ("allOf", &schema.all_of),
        ("anyOf", &schema.any_of),
        ("oneOf", &schema.one_of),
    ] {
        for (index, branch) in branches.iter().enumerate() {
            if let Some(found) = unknown_keyword_pointer(
                branch,
                &child_pointer(&child_pointer(pointer, keyword), &index.to_string()),
            ) {
                return Some(found);
            }
        }
    }

    if let Some(items) = schema.items.as_deref()
        && let Some(found) = unknown_keyword_pointer(items, &child_pointer(pointer, "items"))
    {
        return Some(found);
    }

    for (index, branch) in schema.prefix_items.iter().enumerate() {
        if let Some(found) = unknown_keyword_pointer(
            branch,
            &child_pointer(&child_pointer(pointer, "prefixItems"), &index.to_string()),
        ) {
            return Some(found);
        }
    }

    for (name, property) in &schema.properties {
        if let Some(found) = unknown_keyword_pointer(
            property,
            &child_pointer(&child_pointer(pointer, "properties"), name),
        ) {
            return Some(found);
        }
    }

    if let Some(child) = schema.additional_properties.as_ref()
        && let Some(found) =
            unknown_keyword_pointer(child, &child_pointer(pointer, "additionalProperties"))
    {
        return Some(found);
    }
    None
}
