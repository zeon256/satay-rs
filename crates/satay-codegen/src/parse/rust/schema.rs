//! Rust schema choices derived solely from the semantic graph.
use super::LowerError;
use super::{
    constraint::{self, ConstraintInput},
    policy,
};
use crate::ValidationError;
use crate::ident::{type_ident, unique_ident};
use crate::model;
use crate::model::{
    BoolStringMapping, CoordinateDelimiter, EnumFallback, IntegerType, ParseAs, RangeScalar,
    StringCodec, TypeRef,
};
use crate::parse::helpers;
use crate::parse::satay::SatayIdentifier;
use crate::parse::validate::*;
use satay_ir::{
    self as ir, AdditionalProperties, CompositionKind, DecodePolicy, DiagnosticKind,
    IntegerInterpretation, PropertyPolicy, StringConstraints, StringInterpretation, StringSchema,
    TypeExpr,
};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) struct Schemas<'a> {
    pub(super) api: &'a ir::Api,
    stack: Vec<String>,
}

impl<'a> Schemas<'a> {
    pub(super) fn new(api: &'a ir::Api) -> Self {
        Self { api, stack: vec![] }
    }

    pub(super) fn components(&mut self) -> Result<Vec<ValidatedComponent>, LowerError> {
        self.api
            .definitions()
            .map(|(_, definition)| self.component(definition))
            .collect()
    }

    pub(super) fn definition(&self, id: ir::DefinitionId) -> &'a ir::Definition {
        self.api.definition(id).expect("finalized graph reference")
    }

    fn component(&mut self, definition: &ir::Definition) -> Result<ValidatedComponent, LowerError> {
        let context = format!("schema `{}`", definition.source_name);
        let kind = match &definition.schema.ty {
            TypeExpr::Ref(id) => {
                ValidatedComponentKind::Reference(type_ident(&self.definition(*id).source_name))
            }
            TypeExpr::Object(object) if !object.properties.is_empty() => {
                ValidatedComponentKind::Struct(self.fields(&object.properties, &context)?)
            }
            TypeExpr::Composition(composition) if composition.kind == CompositionKind::AllOf => {
                let fields =
                    self.all_of(&definition.schema, &context, Some(&definition.source_name))?;
                ValidatedComponentKind::Struct(fields)
            }
            _ => ValidatedComponentKind::Type(self.value(&definition.schema, &context)?),
        };
        let description =
            definition
                .schema
                .annotations
                .description
                .clone()
                .or_else(|| match &kind {
                    ValidatedComponentKind::Type(ty) => ty.description.clone(),
                    _ => None,
                });
        Ok(ValidatedComponent {
            schema_name: definition.source_name.clone(),
            description,
            kind,
        })
    }

    /// Fold projection absence into Rust Option without changing semantic nullability.
    pub(super) fn projected_value(
        &mut self,
        projection: &ir::ResponseProjection,
        context: &str,
    ) -> Result<ValidatedType, LowerError> {
        let mut output = self.value(&projection.output, context)?;
        output.nullable |= !projection.unwrap_required;
        if let Some(required) = projection.map_required {
            let ValidatedTypeKind::Array(item) = &mut output.kind else {
                unreachable!("projected output lowers to an array")
            };
            item.nullable |= !required;
        }
        Ok(output)
    }

    #[allow(clippy::too_many_lines)] // Keep ordered schema-policy checks together.
    pub(super) fn value(
        &mut self,
        value: &ir::SchemaUse,
        context: &str,
    ) -> Result<ValidatedType, LowerError> {
        let input = constraints(value);
        let mut nullable = value.nullable;
        let mut description = value.annotations.description.clone();
        let kind = match &value.ty {
            TypeExpr::Invalid(diagnostic) => return Err(diagnostic.clone().into()),
            TypeExpr::Ref(id) => {
                if description.is_none() {
                    let mut seen = vec![];
                    let mut id = *id;
                    loop {
                        if seen.contains(&id) {
                            break;
                        }
                        seen.push(id);
                        let target = self.definition(id);
                        if target.schema.annotations.description.is_some() {
                            description = target.schema.annotations.description.clone();
                            break;
                        }
                        if let TypeExpr::Ref(next) = target.schema.ty {
                            id = next;
                        } else {
                            break;
                        }
                    }
                }
                ValidatedTypeKind::Named(type_ident(&self.definition(*id).source_name))
            }
            TypeExpr::String(string) => {
                if let Some(values) = effective_enum(string) {
                    ValidatedTypeKind::Enum(Self::enum_type(
                        string,
                        values,
                        EnumFallback::None,
                        context,
                    )?)
                } else {
                    match &string.interpretation {
                        StringInterpretation::Plain
                            if value.annotations.format.as_deref() == Some("unixtime") =>
                        {
                            ValidatedTypeKind::ParsedString(StringCodec::Standard(
                                ParseAs::UnixTime,
                            ))
                        }
                        StringInterpretation::Plain
                            if value.annotations.format.as_deref() == Some("uri") =>
                        {
                            constraint::reject_keyword(
                                input.pattern.is_some(),
                                "pattern",
                                context,
                            )?;
                            constraint::reject_keyword(
                                input.min_length.is_some(),
                                "minLength",
                                context,
                            )?;
                            constraint::reject_keyword(
                                input.max_length.is_some(),
                                "maxLength",
                                context,
                            )?;
                            ValidatedTypeKind::ParsedString(StringCodec::Standard(ParseAs::Url))
                        }
                        StringInterpretation::Plain => ValidatedTypeKind::String,
                        StringInterpretation::Scalar(scalar) => ValidatedTypeKind::ParsedString(
                            StringCodec::Standard(parse_as(*scalar)),
                        ),
                        StringInterpretation::MappedBool(mapping) => {
                            ValidatedTypeKind::ParsedString(StringCodec::MappedBool(
                                BoolStringMapping::try_new(
                                    mapping.true_values().to_vec(),
                                    mapping.false_values().to_vec(),
                                    mapping.unknown_as(),
                                )
                                .expect("checked semantic boolean mapping"),
                            ))
                        }
                        StringInterpretation::IntegerRange {
                            representation,
                            bounds,
                        } => {
                            let input = numeric_input(bounds, value.annotations.format.clone());
                            ValidatedTypeKind::Range(RangeScalar::Integer(
                                constraint::parse_integer_type(
                                    &input,
                                    context,
                                    integer_representation(*representation),
                                )?,
                            ))
                        }
                        StringInterpretation::NumberRange { .. } => {
                            ValidatedTypeKind::Range(match value.annotations.format.as_deref() {
                                Some("float") => RangeScalar::F32,
                                Some("double") | None => RangeScalar::F64,
                                Some(format) => {
                                    return Err(ValidationError::UnsupportedNumberFormat {
                                        context: context.to_owned(),
                                        format: format.to_owned(),
                                    }
                                    .into());
                                }
                            })
                        }
                        StringInterpretation::Coordinates(selector) => {
                            ValidatedTypeKind::Coordinates(self.coordinates(selector, context)?)
                        }
                    }
                }
            }
            TypeExpr::Integer(integer) => match integer.interpretation {
                IntegerInterpretation::Bool => ValidatedTypeKind::ParsedInteger(ParseAs::Bool),
                IntegerInterpretation::Numeric { .. }
                    if value.annotations.format.as_deref() == Some("unixtime") =>
                {
                    ValidatedTypeKind::ParsedInteger(ParseAs::UnixTime)
                }
                IntegerInterpretation::Numeric { representation } => {
                    ValidatedTypeKind::Integer(constraint::parse_integer_type(
                        &input,
                        context,
                        integer_representation(representation),
                    )?)
                }
            },
            TypeExpr::Number(_) => match value.annotations.format.as_deref() {
                Some("float") => ValidatedTypeKind::F32,
                Some("double") | None => ValidatedTypeKind::F64,
                Some(format) => {
                    return Err(ValidationError::UnsupportedNumberFormat {
                        context: context.to_owned(),
                        format: format.to_owned(),
                    }
                    .into());
                }
            },
            TypeExpr::Boolean => ValidatedTypeKind::Bool,
            TypeExpr::AnyJson => ValidatedTypeKind::JsonValue,
            TypeExpr::Null => {
                return Err(ValidationError::UnsupportedSchemaType {
                    context: context.to_owned(),
                    kind: "null".to_owned(),
                }
                .into());
            }
            TypeExpr::Array(array) => ValidatedTypeKind::Array(Box::new(
                self.value(&array.items, &format!("{context} items"))?,
            )),
            TypeExpr::Object(object) => {
                if !object.properties.is_empty() {
                    return Err(ValidationError::InlineObjectSchema {
                        context: context.to_owned(),
                    }
                    .into());
                }
                let item = match &object.additional_properties {
                    AdditionalProperties::Allowed => ValidatedType {
                        kind: ValidatedTypeKind::JsonValue,
                        nullable: false,
                        validation: None,
                        description: None,
                    },
                    AdditionalProperties::Schema(item) => {
                        self.value(item, &format!("{context} additionalProperties"))?
                    }
                    _ => {
                        return Err(ValidationError::UnsupportedMapObjectSchema {
                            context: context.to_owned(),
                        }
                        .into());
                    }
                };
                ValidatedTypeKind::Map(Box::new(item))
            }
            TypeExpr::Composition(composition) if composition.kind == CompositionKind::AllOf => {
                if let [branch] = composition.branches.as_slice()
                    && let TypeExpr::Ref(id) = branch.ty
                    && !is_struct(&self.definition(id).schema)
                {
                    let mut ty = self.value(branch, context)?;
                    if description.is_some() {
                        ty.description = description;
                    }
                    return Ok(ty);
                }
                ValidatedTypeKind::InlineStruct(self.all_of(value, context, None)?)
            }
            TypeExpr::Composition(composition) => {
                let (kind, union_nullable) = self.union(composition, context)?;
                if description.is_none()
                    && matches!(&kind, ValidatedTypeKind::Enum(enumeration) if enumeration.fallback == EnumFallback::OtherString)
                {
                    description = composition.branches.iter().filter(|branch| matches!(&branch.ty, TypeExpr::String(string) if effective_enum(string).is_some())).find_map(|branch| branch.annotations.description.clone());
                }
                nullable |= union_nullable;
                kind
            }
        };
        let base = match &kind {
            ValidatedTypeKind::String => Some(TypeRef::String),
            ValidatedTypeKind::Integer(integer) => Some(TypeRef::Integer(*integer)),
            ValidatedTypeKind::F32 => Some(TypeRef::F32),
            ValidatedTypeKind::F64 => Some(TypeRef::F64),
            ValidatedTypeKind::Array(_) => Some(TypeRef::Array(Box::new(TypeRef::Bool))),
            _ => None,
        };
        let validation = base
            .map(|base| constraint::parse_validation(&input, &base, context))
            .transpose()?
            .flatten();
        Ok(ValidatedType {
            kind,
            nullable,
            validation,
            description,
        })
    }

    fn enum_type(
        string: &ir::StringSchema,
        values: Vec<String>,
        fallback: EnumFallback,
        context: &str,
    ) -> Result<model::Enum, LowerError> {
        let explicit = string
            .enum_variants
            .iter()
            .map(|variant| (variant.wire_value.clone(), variant.requested_name.clone()))
            .collect();
        Ok(policy::validated_enum(
            &values.into_iter().map(Value::String).collect::<Vec<_>>(),
            &explicit,
            fallback,
            context,
        )?)
    }

    fn fields(
        &mut self,
        properties: &[ir::Property],
        context: &str,
    ) -> Result<Vec<ValidatedField>, LowerError> {
        let fields = self.field_results(properties, context)?;
        policy::validate_rust_field_identifier_collisions(context, &fields)?;
        Ok(fields)
    }

    fn field_results(
        &mut self,
        properties: &[ir::Property],
        context: &str,
    ) -> Result<Vec<ValidatedField>, LowerError> {
        let mut fields = vec![];
        for property in properties {
            let field_context = helpers::property_context(context, &property.wire_name);
            let PropertyPolicy::Included {
                identifier,
                decoding,
            } = &property.policy
            else {
                continue;
            };
            let ty = self.value(&property.value, &field_context)?;
            let description = ty.description.clone();
            let value = match decoding {
                DecodePolicy::PropagateError => ValidatedFieldValue::Strict(ty),
                DecodePolicy::ErrorAsAbsent => ValidatedFieldValue::Lossy(ty),
                DecodePolicy::SentinelAsAbsent(values) => {
                    let ty = ValidatedParsedString::try_from_type(ty).map_err(|_| {
                        ValidationError::SatayNoneIfRequiresParsedString {
                            context: field_context.clone(),
                        }
                    })?;
                    ValidatedFieldValue::SentinelParsedString {
                        ty,
                        sentinels: NonEmptySentinels::new(values.values().to_vec())
                            .expect("checked semantic sentinels"),
                    }
                }
            };
            fields.push(ValidatedField {
                wire_name: property.wire_name.clone(),
                description,
                identifier: identifier
                    .as_ref()
                    .map(|words| SatayIdentifier::from_words(words.clone())),
                required: property.required,
                value,
            });
        }
        Ok(fields)
    }

    fn all_of(
        &mut self,
        value: &ir::SchemaUse,
        context: &str,
        name: Option<&str>,
    ) -> Result<Vec<ValidatedField>, LowerError> {
        if let Some(name) = name {
            self.push_all_of(name)?;
        }
        let mut fields = vec![];
        let result = self.collect_all_of(value, context, &mut BTreeSet::new(), &mut fields);
        if name.is_some() {
            self.stack.pop();
        }
        result?;
        policy::validate_rust_field_identifier_collisions(context, &fields)?;
        Ok(fields)
    }

    fn push_all_of(&mut self, name: &str) -> Result<(), LowerError> {
        if let Some(index) = self.stack.iter().position(|entry| entry == name) {
            return Err(ValidationError::RecursiveAllOf {
                context: format!("schema `{}`", self.stack[index]),
                schema: name.to_owned(),
            }
            .into());
        }
        self.stack.push(name.to_owned());
        Ok(())
    }

    fn collect_all_of(
        &mut self,
        value: &ir::SchemaUse,
        context: &str,
        used: &mut BTreeSet<String>,
        fields: &mut Vec<ValidatedField>,
    ) -> Result<(), LowerError> {
        let TypeExpr::Composition(composition) = &value.ty else {
            unreachable!("allOf dispatch")
        };
        for (index, branch) in composition.branches.iter().enumerate() {
            self.collect_all_of_branch(branch, context, context, index, used, fields)?;
        }
        Ok(())
    }

    fn collect_all_of_branch(
        &mut self,
        value: &ir::SchemaUse,
        field_context: &str,
        context: &str,
        index: usize,
        used: &mut BTreeSet<String>,
        fields: &mut Vec<ValidatedField>,
    ) -> Result<(), LowerError> {
        match &value.ty {
            TypeExpr::Ref(id) => {
                let definition = self.definition(*id);
                self.push_all_of(&definition.source_name)?;
                let result = self.collect_all_of_branch(
                    &definition.schema,
                    &format!("schema `{}`", definition.source_name),
                    context,
                    index,
                    used,
                    fields,
                );
                self.stack.pop();
                result
            }
            TypeExpr::Composition(composition) if composition.kind == CompositionKind::AllOf => {
                self.collect_all_of(value, field_context, used, fields)
            }
            TypeExpr::Object(object) if !object.properties.is_empty() => {
                // Keep ignored wire names until every branch has been merged.
                // Field identifier collisions are checked only after expansion.
                let branch_fields = self.field_results(&object.properties, field_context)?;
                for property in &object.properties {
                    if !used.insert(property.wire_name.clone()) {
                        return Err(ValidationError::DuplicateAllOfProperty {
                            context: context.to_owned(),
                            property: property.wire_name.clone(),
                        }
                        .into());
                    }
                }
                fields.extend(branch_fields);
                Ok(())
            }
            TypeExpr::Invalid(diagnostic) => Err(diagnostic.clone().into()),
            _ => Err(ValidationError::UnsupportedAllOfBranch {
                context: context.to_owned(),
                index,
            }
            .into()),
        }
    }

    fn coordinates(
        &mut self,
        selector: &ir::CoordinatesInterpretation,
        context: &str,
    ) -> Result<ValidatedCoordinates, LowerError> {
        let definition = self.coordinate_target(selector.target(), context)?;
        let name = &definition.source_name;
        let marker = format!("coordinates:{name}");
        if self.stack.contains(&marker) {
            return Err(invalid_coordinates(
                context,
                format!("target `{name}` recursively uses the coordinate codec"),
            ));
        }
        self.stack.push(marker);
        let component = self.component(definition);
        self.stack.pop();
        let ValidatedComponentKind::Struct(fields) = component?.kind else {
            return Err(invalid_coordinates(
                context,
                format!("target `{name}` must be a generated object"),
            ));
        };
        if fields.len() != 2 {
            return Err(invalid_coordinates(
                context,
                format!("target `{name}` must generate precisely the two selected fields"),
            ));
        }
        let indices = self.coordinate_field_indices(&fields, selector, context, name)?;
        Ok(ValidatedCoordinates::from_semantic(
            type_ident(name),
            indices,
            CoordinateDelimiter::new(selector.delimiter().to_owned()).expect("checked delimiter"),
        ))
    }

    fn coordinate_target(
        &self,
        target: ir::DefinitionId,
        context: &str,
    ) -> Result<&'a ir::Definition, LowerError> {
        let mut definition = self.definition(target);
        let mut seen = vec![];
        while let TypeExpr::Ref(id) = definition.schema.ty {
            if seen.contains(&id) {
                return Err(invalid_coordinates(
                    context,
                    format!(
                        "target `{}` contains a reference cycle",
                        definition.source_name
                    ),
                ));
            }
            seen.push(id);
            definition = self.definition(id);
        }
        let name = &definition.source_name;
        if definition.schema.nullable || !is_struct(&definition.schema) {
            return Err(invalid_coordinates(
                context,
                format!("target `{name}` must be a nonnullable generated object"),
            ));
        }
        if let TypeExpr::Object(object) = &definition.schema.ty
            && object.properties.len() != 2
        {
            return Err(invalid_coordinates(
                context,
                format!("target `{name}` must declare precisely the two selected fields"),
            ));
        }
        Ok(definition)
    }

    fn coordinate_field_indices(
        &mut self,
        fields: &[ValidatedField],
        selector: &ir::CoordinatesInterpretation,
        context: &str,
        name: &str,
    ) -> Result<[usize; 2], LowerError> {
        let mut indices = [0; 2];
        for (output_index, wire_name) in selector.fields().iter().enumerate() {
            let (index, field) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| &field.wire_name == wire_name)
                .ok_or_else(|| {
                    invalid_coordinates(
                        context,
                        format!("target `{name}` has no field `{wire_name}`"),
                    )
                })?;
            if !field.required || !matches!(field.value, ValidatedFieldValue::Strict(_)) {
                return Err(invalid_coordinates(
                    context,
                    format!(
                        "target field `{name}.{wire_name}` must be required with strict numeric decoding"
                    ),
                ));
            }
            let mut ty = field.value.ty().clone();
            let mut seen = BTreeSet::new();
            loop {
                if ty.nullable {
                    break;
                }
                let ValidatedTypeKind::Named(ref rust_name) = ty.kind else {
                    break;
                };
                if !seen.insert(rust_name.clone()) {
                    break;
                }
                let definition = self
                    .api
                    .definitions()
                    .find(|(_, definition)| type_ident(&definition.source_name) == *rust_name)
                    .expect("validated component name")
                    .1;
                if definition.schema.nullable
                    || !matches!(definition.schema.ty, TypeExpr::Number(_) | TypeExpr::Ref(_))
                {
                    break;
                }
                ty = self.value(&definition.schema, context)?;
            }
            if ty.nullable || !matches!(ty.kind, ValidatedTypeKind::F32 | ValidatedTypeKind::F64) {
                return Err(invalid_coordinates(
                    &format!("{context} target field `{name}.{wire_name}`"),
                    "selected fields must resolve to nonnullable f32/f64 numbers",
                ));
            }
            indices[output_index] = index;
        }
        Ok(indices)
    }

    #[allow(clippy::too_many_lines)] // Keep ordered schema-policy checks together.
    fn union(
        &mut self,
        composition: &ir::CompositionSchema,
        context: &str,
    ) -> Result<(ValidatedTypeKind, bool), LowerError> {
        if let Some(discriminator) = &composition.discriminator {
            return self
                .discriminator(composition, discriminator, context)
                .map(|union| (ValidatedTypeKind::AnyOf(union), false));
        }
        if composition.kind == CompositionKind::AnyOf && composition.branches.len() >= 2 {
            let mut open = 0;
            let mut eligible = true;
            let mut merged = StringSchema::default();
            let mut values = vec![];
            for branch in &composition.branches {
                let TypeExpr::String(string) = &branch.ty else {
                    eligible = false;
                    break;
                };
                if let Some(contribution) = effective_enum(string) {
                    values.extend(contribution);
                    merged.enum_variants.extend(string.enum_variants.clone());
                } else if !branch.nullable
                    && branch.annotations.format.is_none()
                    && string.constraints == StringConstraints::default()
                    && string.interpretation == StringInterpretation::Plain
                {
                    open += 1;
                } else {
                    eligible = false;
                    break;
                }
            }
            if eligible && open == 1 && !values.is_empty() {
                let mut seen = BTreeSet::new();
                for value in &values {
                    if !seen.insert(value) {
                        return Err(ValidationError::DuplicateOpenStringEnumValue {
                            context: context.to_owned(),
                            value: value.clone(),
                        }
                        .into());
                    }
                }
                let mut seen = BTreeSet::new();
                for variant in &merged.enum_variants {
                    if !seen.insert(&variant.requested_name) {
                        return Err(ValidationError::DuplicateSatayEnumVariantName {
                            context: context.to_owned(),
                            rust_name: variant.requested_name.clone(),
                        }
                        .into());
                    }
                }
                return Ok((
                    ValidatedTypeKind::Enum(Self::enum_type(
                        &merged,
                        values,
                        EnumFallback::OtherString,
                        context,
                    )?),
                    false,
                ));
            }
        }
        let keyword = if composition.kind == CompositionKind::OneOf {
            "oneOf"
        } else {
            "anyOf"
        };
        let mut variants = vec![];
        let mut indexes = vec![];
        let mut used = BTreeSet::new();
        let mut nullable = false;
        for (index, branch) in composition.branches.iter().enumerate() {
            let error = || {
                if keyword == "oneOf" {
                    ValidationError::UnsupportedOneOfBranch {
                        context: context.to_owned(),
                        index,
                    }
                } else {
                    ValidationError::UnsupportedAnyOfBranch {
                        context: context.to_owned(),
                        index,
                    }
                }
            };
            if let TypeExpr::Invalid(diagnostic) = &branch.ty {
                if matches!(
                    diagnostic.kind,
                    DiagnosticKind::UnsupportedRefSiblingKeyword { .. }
                        | DiagnosticKind::InvalidExtension { .. }
                        | DiagnosticKind::DiscriminatorMappingValueMismatch { .. }
                ) {
                    return Err(diagnostic.clone().into());
                }
                if matches!(
                    diagnostic.kind,
                    DiagnosticKind::SatayTreatErrorAsNoneRequiresObjectProperty { .. }
                ) {
                    return Err(
                        ValidationError::SatayTreatErrorAsNoneRequiresObjectProperty {
                            context: context.to_owned(),
                        }
                        .into(),
                    );
                }
                return Err(error().into());
            }
            if branch
                .annotations
                .const_value
                .as_ref()
                .is_some_and(|value| !value.is_string())
            {
                return Err(error().into());
            }
            if matches!(branch.ty, TypeExpr::Null) {
                if nullable {
                    return Err(ValidationError::DuplicateUnionNullBranch {
                        context: context.to_owned(),
                        keyword,
                        index,
                    }
                    .into());
                }
                nullable = true;
                continue;
            }
            let branch = match &branch.ty {
                TypeExpr::Composition(inner)
                    if inner.kind == CompositionKind::AllOf
                        && inner.branches.len() == 1
                        && matches!(inner.branches[0].ty, TypeExpr::Ref(_)) =>
                {
                    &inner.branches[0]
                }
                _ => branch,
            };
            let (name, kind) = if let TypeExpr::Ref(id) = branch.ty {
                let schema_name = self.definition(id).source_name.clone();
                let type_name = type_ident(&schema_name);
                (
                    type_name.clone(),
                    ValidatedUnionVariantKind::Reference {
                        schema_name,
                        type_name,
                    },
                )
            } else {
                if branch.nullable {
                    return Err(error().into());
                }
                let name = match &branch.ty {
                    TypeExpr::String(_) => "String",
                    TypeExpr::Integer(_) => "Integer",
                    TypeExpr::Number(_) => "Number",
                    TypeExpr::Boolean => "Boolean",
                    TypeExpr::Array(_) => "Array",
                    TypeExpr::Object(object) if object.properties.is_empty() => "Map",
                    TypeExpr::Composition(inner)
                        if inner.discriminator.is_some()
                            && inner.kind == CompositionKind::OneOf =>
                    {
                        "Union"
                    }
                    _ => return Err(error().into()),
                };
                let ty = if matches!(&branch.ty, TypeExpr::Composition(inner) if inner.discriminator.is_some())
                {
                    self.value(branch, &format!("{context}.{keyword}[{index}]"))?
                } else {
                    self.value(branch, context).map_err(|_| error())?
                };
                if let ValidatedTypeKind::AnyOf(union) = &ty.kind
                    && union.variants.len() == 1
                    && union
                        .tag
                        .as_ref()
                        .is_some_and(|tag| tag.style == ValidatedUnionTagStyle::EmbeddedField)
                {
                    let mut variant = union.variants[0].clone();
                    variant.rust_name = unique_ident(variant.rust_name, &mut used);
                    variants.push(variant);
                    indexes.push(index);
                    continue;
                }
                let name =
                    policy::inline_union_enum_variant_name(&ty).unwrap_or_else(|| name.to_owned());
                (name, ValidatedUnionVariantKind::Inline(ty))
            };
            let variant = ValidatedUnionVariant {
                rust_name: unique_ident(name, &mut used),
                kind,
                tag_value: None,
            };
            for (previous, shadowed_by) in variants.iter().zip(&indexes) {
                if policy::plain_union_branch_shadows(previous, &variant) {
                    return Err(ValidationError::ShadowedUnionBranch {
                        context: context.to_owned(),
                        keyword,
                        index,
                        shadowed_by: *shadowed_by,
                    }
                    .into());
                }
            }
            variants.push(variant);
            indexes.push(index);
        }
        if variants.is_empty() {
            return Err(ValidationError::NullableUnionWithoutVariants {
                context: context.to_owned(),
                keyword,
            }
            .into());
        }
        if variants.len() == 1
            && let ValidatedUnionVariantKind::Inline(ty) = &variants[0].kind
            && matches!(
                ty.kind,
                ValidatedTypeKind::AnyOf(_) | ValidatedTypeKind::Map(_)
            )
        {
            return Ok((ty.kind.clone(), nullable));
        }
        Ok((
            ValidatedTypeKind::AnyOf(ValidatedUnion {
                variants,
                tag: None,
            }),
            nullable,
        ))
    }

    #[allow(clippy::too_many_lines)] // Keep ordered schema-policy checks together.
    fn discriminator(
        &mut self,
        composition: &ir::CompositionSchema,
        discriminator: &ir::Discriminator,
        context: &str,
    ) -> Result<ValidatedUnion, LowerError> {
        let keyword = if composition.kind == CompositionKind::OneOf {
            "oneOf"
        } else {
            "anyOf"
        };
        let mut branches = vec![];
        for (index, branch) in composition.branches.iter().enumerate() {
            if let TypeExpr::Invalid(diagnostic) = &branch.ty {
                return Err(diagnostic.clone().into());
            }
            let TypeExpr::Ref(id) = branch.ty else {
                return Err(ValidationError::UnsupportedDiscriminatorBranch {
                    context: context.to_owned(),
                    keyword,
                    index,
                }
                .into());
            };
            let definition = self.definition(id);
            if !is_struct(&definition.schema) || definition.schema.nullable {
                return Err(ValidationError::DiscriminatorBranchNotObject {
                    context: context.to_owned(),
                    schema: definition.source_name.clone(),
                }
                .into());
            }
            let all_of = matches!(&definition.schema.ty, TypeExpr::Composition(composition) if composition.kind == CompositionKind::AllOf);
            if !all_of {
                if let Some(index) = self
                    .stack
                    .iter()
                    .position(|entry| entry == &definition.source_name)
                {
                    return Err(ValidationError::RecursiveDiscriminatorBranch {
                        context: format!("schema `{}`", self.stack[index]),
                        schema: definition.source_name.clone(),
                    }
                    .into());
                }
                self.stack.push(definition.source_name.clone());
            }
            let component = self.component(definition);
            if !all_of {
                self.stack.pop();
            }
            let component = component.map_err(|error| match &error {
                LowerError::Frontend(diagnostic)
                    if matches!(diagnostic.kind, DiagnosticKind::NonStringEnumValue { .. }) =>
                {
                    ValidationError::InvalidDiscriminatorProperty {
                        context: context.to_owned(),
                        schema: definition.source_name.clone(),
                        property: discriminator.property_name.clone(),
                        expected: "a strict, required, non-null singleton string enum or string const",
                    }
                    .into()
                }
                _ => error,
            })?;
            let ValidatedComponentKind::Struct(fields) = component.kind else {
                unreachable!("checked object component")
            };
            let field = fields
                .iter()
                .find(|field| field.wire_name == discriminator.property_name);
            let tag = match field {
                None => None,
                Some(field) => {
                    if let ValidatedTypeKind::Enum(enumeration) = &field.value.ty().kind
                        && field.required
                        && matches!(field.value, ValidatedFieldValue::Strict(_))
                        && !field.value.ty().nullable
                        && enumeration.variants.len() == 1
                        && enumeration.fallback == EnumFallback::None
                    {
                        Some(enumeration.variants[0].wire_name.clone())
                    } else {
                        return Err(ValidationError::InvalidDiscriminatorProperty { context: context.to_owned(), schema: definition.source_name.clone(), property: discriminator.property_name.clone(), expected: "a strict, required, non-null singleton string enum or string const" }.into());
                    }
                }
            };
            branches.push((id, definition.source_name.clone(), tag));
        }
        let embedded = branches.iter().any(|(_, _, tag)| tag.is_some());
        let mut variants = vec![];
        let mut used = BTreeSet::new();
        let mut tags = BTreeSet::new();
        for (id, schema_name, tag) in branches {
            if embedded && tag.is_none() {
                return Err(ValidationError::InvalidDiscriminatorProperty {
                    context: context.to_owned(),
                    schema: schema_name,
                    property: discriminator.property_name.clone(),
                    expected: "present on every branch when any branch contains it",
                }
                .into());
            }
            let mapped = discriminator
                .mappings
                .iter()
                .find(|mapping| mapping.target == id)
                .map(|mapping| mapping.wire_value.clone());
            if discriminator
                .mappings
                .iter()
                .filter(|mapping| mapping.target == id)
                .count()
                > 1
            {
                return Err(ValidationError::DuplicateDiscriminatorMapping {
                    context: context.to_owned(),
                    schema: schema_name,
                }
                .into());
            }
            if let (Some(actual), Some(value)) = (&tag, &mapped)
                && actual != value
            {
                return Err(ValidationError::DiscriminatorMappingValueMismatch {
                    context: context.to_owned(),
                    schema: schema_name,
                    value: value.clone(),
                    actual: actual.clone(),
                }
                .into());
            }
            let tag_value = tag.or(mapped).unwrap_or_else(|| schema_name.clone());
            if !tags.insert(tag_value.clone()) {
                return Err(ValidationError::DuplicateDiscriminatorValue {
                    context: context.to_owned(),
                    value: tag_value,
                }
                .into());
            }
            let type_name = type_ident(&schema_name);
            variants.push(ValidatedUnionVariant {
                rust_name: unique_ident(type_name.clone(), &mut used),
                kind: ValidatedUnionVariantKind::Reference {
                    type_name,
                    schema_name,
                },
                tag_value: (!embedded).then_some(tag_value),
            });
        }
        Ok(ValidatedUnion {
            variants,
            tag: Some(ValidatedUnionTag {
                property_name: discriminator.property_name.clone(),
                style: if embedded {
                    ValidatedUnionTagStyle::EmbeddedField
                } else {
                    ValidatedUnionTagStyle::InternallyTagged
                },
            }),
        })
    }
}

fn is_struct(value: &ir::SchemaUse) -> bool {
    matches!(&value.ty, TypeExpr::Object(object) if !object.properties.is_empty())
        || matches!(&value.ty, TypeExpr::Composition(composition) if composition.kind == CompositionKind::AllOf)
}

fn invalid_coordinates(context: &str, reason: impl Into<String>) -> LowerError {
    ValidationError::InvalidSatayCoordinates {
        context: context.to_owned(),
        reason: reason.into(),
    }
    .into()
}

fn effective_enum(string: &ir::StringSchema) -> Option<Vec<String>> {
    string
        .const_value
        .as_ref()
        .map(|value| vec![value.clone()])
        .or_else(|| {
            string
                .enum_values
                .clone()
                .filter(|values| !values.is_empty())
        })
}

pub(super) fn constraints(value: &ir::SchemaUse) -> ConstraintInput {
    let mut input = ConstraintInput {
        format: value.annotations.format.clone(),
        ..ConstraintInput::default()
    };
    match &value.ty {
        TypeExpr::Integer(integer) => input = numeric_input(&integer.constraints, input.format),
        TypeExpr::Number(number) => input = numeric_input(&number.constraints, input.format),
        TypeExpr::String(string) => {
            input.min_length = string.constraints.min_length;
            input.max_length = string.constraints.max_length;
            input.pattern = string.constraints.pattern.clone();
        }
        TypeExpr::Array(array) => {
            input.min_items = array.constraints.min_items;
            input.max_items = array.constraints.max_items;
            input.unique_items = Some(array.constraints.unique_items);
        }
        _ => {}
    }
    input
}

fn numeric_input(bounds: &ir::NumericConstraints, format: Option<String>) -> ConstraintInput {
    let mut input = ConstraintInput {
        format,
        ..ConstraintInput::default()
    };
    if let Some(declared) = &bounds.declared {
        input.minimum = declared.minimum.clone();
        input.maximum = declared.maximum.clone();
        input.exclusive_minimum = declared.exclusive_minimum.clone();
        input.exclusive_maximum = declared.exclusive_maximum.clone();
        input.multiple_of = declared.multiple_of.clone();
        return input;
    }
    if let Some(bound) = &bounds.minimum {
        if bound.exclusive {
            input.exclusive_minimum = Some(bound.value.clone());
        } else {
            input.minimum = Some(bound.value.clone());
        }
    }
    if let Some(bound) = &bounds.maximum {
        if bound.exclusive {
            input.exclusive_maximum = Some(bound.value.clone());
        } else {
            input.maximum = Some(bound.value.clone());
        }
    }
    input
}

fn integer_representation(value: Option<ir::IntegerRepresentation>) -> Option<IntegerType> {
    use ir::IntegerRepresentation as I;
    Some(match value? {
        I::Auto => return None,
        I::U8 => IntegerType::U8,
        I::U16 => IntegerType::U16,
        I::U32 => IntegerType::U32,
        I::U64 => IntegerType::U64,
        I::I8 => IntegerType::I8,
        I::I16 => IntegerType::I16,
        I::I32 => IntegerType::I32,
        I::I64 => IntegerType::I64,
    })
}

fn parse_as(value: ir::StringScalar) -> ParseAs {
    use ir::StringScalar as S;
    match value {
        S::U8 => ParseAs::U8,
        S::U16 => ParseAs::U16,
        S::U32 => ParseAs::U32,
        S::U64 => ParseAs::U64,
        S::I8 => ParseAs::I8,
        S::I16 => ParseAs::I16,
        S::I32 => ParseAs::I32,
        S::I64 => ParseAs::I64,
        S::F32 => ParseAs::F32,
        S::F64 => ParseAs::F64,
        S::Bool => ParseAs::Bool,
        S::Date => ParseAs::Date,
        S::NaiveDatetime => ParseAs::NaiveDateTime,
        S::OffsetDatetime => ParseAs::OffsetDateTime,
        S::Time => ParseAs::Time,
    }
}
