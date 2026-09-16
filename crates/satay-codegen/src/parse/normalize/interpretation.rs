//! Single-read `x-satay` option normalization for the IR frontend.
//!
//! Every schema use reads its `x-satay` extension exactly once through
//! [`NormalizeContext::use_options_impl`], which validates option placement,
//! derives the declared interpretation, and produces the property policy.
//! Reference targets, alias chains, and coordinate targets resolve over
//! original source identities; no Rust representation decision is made here,
//! so the retained interpretation stays distinguishable from a format-derived
//! backend choice.

use core::slice;
use std::collections::{BTreeMap, BTreeSet};

use oas3::spec::{
    ObjectSchema as OasObjectSchema, Schema as OasSchema, SchemaType as OasSchemaType,
};
use satay_ir::{
    BoolMapping, CoordinatesInterpretation, DecodePolicy, EnumVariantName, IntegerInterpretation,
    IntegerRepresentation, InterpretationError, PropertyPolicy, SentinelValues,
    StringInterpretation, StringScalar,
};
use serde_json::Value as JsonValue;

use super::constraint::numeric_constraints;
use super::source::{child_pointer, source_ref};
use super::{NormalizeContext, NormalizeError, SchemaPosition, ValidationErrorExt};
use crate::error::ValidationError;
use crate::parse::reference::{schema_component_ref, schema_type_and_nullable, schema_type_wire};
use crate::parse::satay::{
    SatayIntegerTypeWire, SatayParseAsWire, SataySchemaOptions, schema_options,
};
use crate::parse::validate::satay::{reject_options_with_ignore, reject_property_options_on_value};
use crate::parse::validate::schema::{
    annotation_only_all_of_ref_wrapper, reject_all_of_object_branch_keywords,
    reject_all_of_sibling_keywords, unsupported_reference_schema_keyword,
};

/// One use's declared `x-satay` semantics, read exactly once.
#[derive(Debug, Clone, Default, PartialEq)]
pub(in crate::parse) struct InterpretedUse {
    /// Declared string interpretation.
    pub(in crate::parse) string: StringInterpretation,
    /// Declared integer interpretation.
    pub(in crate::parse) integer: IntegerInterpretation,
    /// Property policy derived from property-local options.
    pub(in crate::parse) policy: PropertyPolicy,
    /// Explicitly requested enum variant names in the options' own order.
    pub(in crate::parse) enum_variants: Vec<EnumVariantName>,
}

impl NormalizeContext<'_, '_> {
    /// Reads one use's `x-satay` options exactly once and validates option
    /// placement, interpretation, and policy against the surrounding schema.
    ///
    /// `pointer` locates the use so extension-level failures can be attached
    /// to the `x-satay` node they concern.
    ///
    /// # Errors
    ///
    /// Returns a structured [`NormalizeError`] for misplaced options, invalid
    /// interpretations, and rejected configurations.
    #[allow(clippy::too_many_lines)]
    pub(super) fn use_options_impl(
        &self,
        schema: &OasObjectSchema,
        schema_type: Option<OasSchemaType>,
        position: SchemaPosition,
        pointer: &str,
        context: &str,
    ) -> Result<InterpretedUse, NormalizeError> {
        if schema.reference.is_some() {
            Self::reject_reference_schema_siblings(schema, context)
                .map_err(|error| error.at(self, pointer))?;
        }
        let Some(options) = schema_options(schema, context)
            .map_err(|error| error.at(self, &child_pointer(pointer, "x-satay")))?
        else {
            return Ok(InterpretedUse::default());
        };
        let location = child_pointer(pointer, "x-satay");
        if schema.reference.is_some()
            && let Some(keyword) =
                reference_satay_keyword(&options, position == SchemaPosition::Property)
        {
            return Err(ValidationError::UnsupportedRefSiblingKeyword {
                context: context.to_owned(),
                keyword: format!("x-satay.{keyword}"),
            }
            .at(self, &location));
        }

        if position == SchemaPosition::Property && !self.recover {
            reject_options_with_ignore(&options, context)
                .map_err(|error| error.at(self, &location))?;
        }
        if position != SchemaPosition::Property {
            reject_property_options_on_value(
                options.treat_error_as_none,
                options.none_if.as_deref(),
                options.ignore,
                options.identifier.as_ref(),
                context,
            )
            .map_err(|error| error.at(self, &location))?;
        }

        let effective = Self::effective_declared_values(schema, schema_type, context)
            .map_err(|error| error.at(self, &child_pointer(pointer, "const")))?;
        if !effective.is_empty() {
            let enum_variants = self.enum_variant_names(&options, effective, &location, context)?;
            if position == SchemaPosition::Property {
                reject_options_with_ignore(&options, context)
                    .map_err(|error| error.at(self, &location))?;
            }
            return Ok(InterpretedUse {
                policy: self.property_policy(&options, position, &location)?,
                enum_variants,
                ..InterpretedUse::default()
            });
        }

        Self::validate_option_placement(&options, schema_type, context)
            .map_err(|error| error.at(self, &location))?;

        let mut string = StringInterpretation::Plain;
        let mut integer = IntegerInterpretation::Numeric {
            representation: integer_representation(options.integer_type),
        };
        let mut parsed_bool = false;
        match options.parse_as {
            Some(SatayParseAsWire::Coordinates) => {
                string = StringInterpretation::Coordinates(self.coordinate_interpretation(
                    schema,
                    schema_type,
                    &options,
                    pointer,
                    context,
                )?);
            }
            Some(wire) => match schema_type {
                Some(OasSchemaType::String) => match string_scalar_from_wire(wire) {
                    Some(scalar) => {
                        string = StringInterpretation::Scalar(scalar);
                        parsed_bool = wire == SatayParseAsWire::Bool;
                    }
                    None => {
                        let integer_range = wire == SatayParseAsWire::IntegerRange;
                        let bounds =
                            numeric_constraints(schema, integer_range, context, self.recover)
                                .map_err(|error| error.at(self, pointer))?;
                        string = if integer_range {
                            StringInterpretation::IntegerRange {
                                representation: integer_representation(options.integer_type),
                                bounds,
                            }
                        } else {
                            StringInterpretation::NumberRange { bounds }
                        };
                    }
                },
                Some(OasSchemaType::Integer) if wire == SatayParseAsWire::Bool => {
                    integer = IntegerInterpretation::Bool;
                }
                other => {
                    return Err(ValidationError::SatayParseAsRequiresString {
                        context: context.to_owned(),
                        parse_as: parse_as_name(wire).to_owned(),
                        kind: other.map(schema_type_wire).unwrap_or("missing").to_owned(),
                    }
                    .at(self, &location));
                }
            },
            None => {}
        }

        if options.none_if.is_some() {
            if !(matches!(
                string,
                StringInterpretation::Scalar(_) | StringInterpretation::Coordinates(_)
            ) || (self.recover
                && schema_type == Some(OasSchemaType::String)
                && schema.format.as_deref() == Some("uri")))
            {
                return Err(ValidationError::SatayNoneIfRequiresParsedString {
                    context: context.to_owned(),
                }
                .at(self, &location));
            }
            if options.treat_error_as_none == Some(true) {
                return Err(ValidationError::ConflictingSatayNoneHandling {
                    context: context.to_owned(),
                }
                .at(self, &location));
            }
        }

        if let Some(mapping) = self.bool_string_mapping(
            &options,
            options.none_if.as_deref().unwrap_or(&[]),
            parsed_bool,
            &location,
            context,
        )? {
            string = StringInterpretation::MappedBool(mapping);
        }

        if position == SchemaPosition::Property {
            reject_options_with_ignore(&options, context)
                .map_err(|error| error.at(self, &location))?;
        }
        Ok(InterpretedUse {
            string,
            integer,
            policy: self.property_policy(&options, position, &location)?,
            enum_variants: vec![],
        })
    }

    /// Validates option placement for a non-enum use.
    ///
    /// # Errors
    ///
    /// Rejects `x-satay.enum-variants` without enum values, coordinate-only
    /// options without the coordinate codec, an empty `none-if` list, and
    /// `integer-type` outside its supported placements.
    fn validate_option_placement(
        options: &SataySchemaOptions,
        schema_type: Option<OasSchemaType>,
        context: &str,
    ) -> Result<(), ValidationError> {
        if options.enum_variants.is_some() {
            return Err(ValidationError::SatayEnumVariantsRequireEnum {
                context: context.to_owned(),
            });
        }
        if options.none_if.as_ref().is_some_and(Vec::is_empty) {
            return Err(ValidationError::EmptySatayNoneIf {
                context: context.to_owned(),
            });
        }

        if options.parse_as == Some(SatayParseAsWire::Coordinates) {
            // The coordinate selector owns its more specific placement errors.
            return Ok(());
        }
        let stray = options
            .target
            .as_ref()
            .map(|_| "target")
            .or_else(|| options.fields.as_ref().map(|_| "fields"))
            .or_else(|| options.delimiter.as_ref().map(|_| "delimiter"));
        if let Some(keyword) = stray {
            return Err(ValidationError::SatayOptionRequiresCoordinates {
                context: context.to_owned(),
                keyword,
            });
        }

        let Some(integer_type) = options.integer_type else {
            return Ok(());
        };
        if schema_type == Some(OasSchemaType::Integer)
            && options.parse_as == Some(SatayParseAsWire::Bool)
        {
            return Err(ValidationError::SatayParseAsBoolWithIntegerType {
                context: context.to_owned(),
                integer_type: integer_type_name(integer_type).to_owned(),
            });
        }
        if schema_type == Some(OasSchemaType::Integer)
            || (schema_type == Some(OasSchemaType::String)
                && options.parse_as == Some(SatayParseAsWire::IntegerRange))
        {
            return Ok(());
        }
        Err(ValidationError::SatayIntegerTypeRequiresInteger {
            context: context.to_owned(),
            integer_type: integer_type_name(integer_type).to_owned(),
            kind: schema_type
                .map(schema_type_wire)
                .unwrap_or("missing")
                .to_owned(),
        })
    }

    /// Derives the declared string interpretation of one use.
    ///
    /// Coordinate targets resolve over source identities; range strings keep
    /// their declared bounds. Formats never create interpretations.
    #[allow(clippy::too_many_lines)]
    fn coordinate_interpretation(
        &self,
        schema: &OasObjectSchema,
        schema_type: Option<OasSchemaType>,
        options: &SataySchemaOptions,
        pointer: &str,
        context: &str,
    ) -> Result<CoordinatesInterpretation, NormalizeError> {
        let location = child_pointer(pointer, "x-satay");
        let invalid = |reason: String| ValidationError::InvalidSatayCoordinates {
            context: context.to_owned(),
            reason,
        };

        if schema_type != Some(OasSchemaType::String) {
            return Err(
                invalid("parse-as `coordinates` requires a string schema".to_owned())
                    .at(self, &location),
            );
        }
        if options.integer_type.is_some() {
            return Err(
                invalid("integer-type is not meaningful for coordinates".to_owned())
                    .at(self, &location),
            );
        }
        for keyword in schema.present_keywords() {
            if !matches!(
                keyword,
                "type"
                    | "title"
                    | "description"
                    | "default"
                    | "deprecated"
                    | "readOnly"
                    | "writeOnly"
                    | "examples"
                    | "example"
            ) && !keyword.starts_with("x-")
            {
                return Err(invalid(format!(
                    "schema keyword `{keyword}` cannot be combined with coordinates"
                ))
                .at(self, &location));
            }
        }
        let Some(target) = options.target.as_ref() else {
            return Err(
                invalid("target with a schema $ref is required".to_owned()).at(self, &location)
            );
        };
        let reference = schema_component_ref(&target.reference)
            .map_err(|source| ValidationError::ResolveReference {
                reference: target.reference.clone(),
                context: context.to_owned(),
                source: Box::new(source),
            })
            .map_err(|error| error.at(self, &location))?;
        let Some([first, second]) = options.fields.as_deref() else {
            return Err(invalid(
                "fields must select exactly two distinct target wire names".to_owned(),
            )
            .at(self, &location));
        };
        if first == second {
            return Err(
                invalid("fields must select two distinct target wire names".to_owned())
                    .at(self, &location),
            );
        }
        let delimiter = options.delimiter.clone().unwrap_or_else(|| " ".to_owned());
        if delimiter.is_empty() {
            return Err(
                invalid("delimiter must be a nonempty literal string".to_owned())
                    .at(self, &location),
            );
        }

        let target_name = reference.name();
        let target_id = match self.definitions.get(target_name) {
            Some(&target_id) => target_id,
            None if self.excluded.contains(target_name) => {
                return Err(NormalizeError::ExcludedDefinition {
                    name: target_name.to_owned(),
                    location: source_ref(self.document_id, &location),
                });
            }
            None => {
                return Err(ValidationError::MissingJsonPointerToken {
                    token: target_name.to_owned(),
                }
                .at(self, &location));
            }
        };
        if self.recover {
            // The Rust stage checks the resolved target in legacy encounter order.
            // Keep the declared alias identity here, including invalid targets,
            // so diagnostics name the terminal generated component correctly.
            return CoordinatesInterpretation::new(
                target_id,
                [first.as_str().to_owned(), second.as_str().to_owned()],
                delimiter,
            )
            .map_err(|source| NormalizeError::Interpretation {
                location: source_ref(self.document_id, &location),
                source,
            });
        }
        let mut declared = BTreeMap::new();
        self.coordinate_object_fields(
            self.component_schema_at(target_name, &location)?,
            &child_pointer("/components/schemas", target_name),
            &mut BTreeSet::from([target_name.to_owned()]),
            &mut declared,
            &location,
            context,
        )?;

        if declared.len() != 2 {
            return Err(invalid(format!(
                "target `{target_name}` must declare precisely the two selected fields"
            ))
            .at(self, &location));
        }

        for selected in [first.as_str(), second.as_str()] {
            let Some(field) = declared.get(selected) else {
                return Err(
                    invalid(format!("target `{target_name}` has no field `{selected}`"))
                        .at(self, &location),
                );
            };
            let field_options = match field.schema {
                OasSchema::Object(object) => schema_options(object, context)
                    .map_err(|error| error.at(self, &child_pointer(&field.pointer, "x-satay")))?,
                OasSchema::Boolean(_) => None,
            };
            let included = field_options
                .as_ref()
                .is_none_or(|options| options.ignore != Some(true));
            let strict = field_options.as_ref().is_none_or(|options| {
                options.treat_error_as_none != Some(true) && options.none_if.is_none()
            });
            if !field.required || !included || !strict {
                return Err(invalid(format!(
                    "target field `{target_name}.{selected}` must be required with strict numeric decoding"
                ))
                .at(self, &location));
            }
            self.coordinate_field_number(field.schema, &location, target_name, selected, context)?;
        }

        CoordinatesInterpretation::new(
            target_id,
            [first.as_str().to_owned(), second.as_str().to_owned()],
            delimiter,
        )
        .map_err(|source| NormalizeError::Interpretation {
            location: source_ref(self.document_id, &location),
            source,
        })
    }

    /// Collects an object's field set without flattening the returned graph.
    /// Active component names are local to this query and released per branch.
    fn coordinate_object_fields<'s>(
        &'s self,
        schema: &'s OasSchema,
        pointer: &str,
        active: &mut BTreeSet<String>,
        declared: &mut BTreeMap<&'s str, DeclaredField<'s>>,
        location: &str,
        context: &str,
    ) -> Result<(), NormalizeError> {
        let invalid = |reason: &str| {
            ValidationError::InvalidSatayCoordinates {
                context: context.to_owned(),
                reason: reason.to_owned(),
            }
            .at(self, location)
        };
        let OasSchema::Object(object) = schema else {
            return Err(invalid("target must resolve to a nonnullable object"));
        };
        if let Some(reference) = object.reference.as_deref() {
            self.reject_alias_reference_siblings(object, pointer, context)?;
            let reference =
                schema_component_ref(reference).map_err(|error| error.at(self, pointer))?;
            let name = reference.name();
            if !active.insert(name.to_owned()) {
                return Err(invalid("target requires recursive allOf flattening"));
            }
            let result = self.coordinate_object_fields(
                self.component_schema_at(name, location)?,
                &child_pointer("/components/schemas", name),
                active,
                declared,
                location,
                context,
            );
            active.remove(name);
            return result;
        }

        let (schema_type, nullable) =
            schema_type_and_nullable(object, context).map_err(|error| error.at(self, pointer))?;
        if nullable
            || !matches!(schema_type, Some(OasSchemaType::Object) | None)
            || !object.any_of.is_empty()
            || !object.one_of.is_empty()
        {
            return Err(invalid("target must resolve to a nonnullable object"));
        }
        if !object.all_of.is_empty() {
            if annotation_only_all_of_ref_wrapper(object).is_none() {
                reject_all_of_sibling_keywords(object, context)
                    .map_err(|error| error.at(self, pointer))?;
            }
            for (index, branch) in object.all_of.iter().enumerate() {
                let branch_pointer =
                    child_pointer(&child_pointer(pointer, "allOf"), &index.to_string());
                if let OasSchema::Object(branch_object) = branch
                    && branch_object.reference.is_none()
                {
                    reject_all_of_object_branch_keywords(branch_object, context, index)
                        .map_err(|error| error.at(self, &branch_pointer))?;
                }
                self.coordinate_object_fields(
                    branch,
                    &branch_pointer,
                    active,
                    declared,
                    location,
                    context,
                )?;
            }
        }
        for (wire_name, schema) in &object.properties {
            let field = DeclaredField {
                required: object.required.contains(wire_name),
                schema,
                pointer: child_pointer(&child_pointer(pointer, "properties"), wire_name),
            };
            if declared.insert(wire_name, field).is_some() {
                return Err(invalid("target has a repeated wire property"));
            }
        }
        Ok(())
    }

    /// Requires one selected target field to resolve to a non-null number.
    fn coordinate_field_number(
        &self,
        schema: &OasSchema,
        location: &str,
        target_name: &str,
        field_name: &str,
        context: &str,
    ) -> Result<(), NormalizeError> {
        let invalid = || {
            ValidationError::InvalidSatayCoordinates {
                context: context.to_owned(),
                reason: format!(
                    "target field `{target_name}.{field_name}` must resolve to a nonnullable number"
                ),
            }
            .at(self, location)
        };
        let mut current = schema;
        let mut visited = BTreeSet::new();
        loop {
            let OasSchema::Object(object) = current else {
                return Err(invalid());
            };
            if let Some(reference) = object
                .reference
                .as_deref()
                .or_else(|| annotation_only_all_of_ref_wrapper(object))
            {
                let reference =
                    schema_component_ref(reference).map_err(|error| error.at(self, location))?;
                let name = reference.name();
                if !visited.insert(name.to_owned()) {
                    return Err(invalid());
                }
                current = self.component_schema_at(name, location)?;
                continue;
            }
            let (field_type, nullable) = schema_type_and_nullable(object, context)
                .map_err(|error| error.at(self, location))?;
            if nullable
                || field_type != Some(OasSchemaType::Number)
                || !object.all_of.is_empty()
                || !object.any_of.is_empty()
                || !object.one_of.is_empty()
            {
                return Err(invalid());
            }
            return Ok(());
        }
    }

    /// Checks ordinary reference siblings before decoding the Satay extension,
    /// preserving the existing reference-first keyword error order.
    fn reject_reference_schema_siblings(
        object: &OasObjectSchema,
        context: &str,
    ) -> Result<(), ValidationError> {
        if let Some(keyword) = unsupported_reference_schema_keyword(object) {
            return Err(ValidationError::UnsupportedRefSiblingKeyword {
                context: context.to_owned(),
                keyword: keyword.to_owned(),
            });
        }
        for extension in object.extensions.keys() {
            if extension != "satay" {
                return Err(ValidationError::UnsupportedRefSiblingKeyword {
                    context: context.to_owned(),
                    keyword: format!("x-{extension}"),
                });
            }
        }
        Ok(())
    }

    /// Rejects sibling keywords and every `x-satay` key on an alias hop.
    fn reject_alias_reference_siblings(
        &self,
        object: &OasObjectSchema,
        location: &str,
        context: &str,
    ) -> Result<(), NormalizeError> {
        Self::reject_reference_schema_siblings(object, context)
            .map_err(|error| error.at(self, location))?;
        let extension_location = child_pointer(location, "x-satay");
        let options = schema_options(object, context)
            .map_err(|error| error.at(self, &extension_location))?
            .unwrap_or_default();
        if let Some(keyword) = reference_satay_keyword(&options, false) {
            return Err(ValidationError::UnsupportedRefSiblingKeyword {
                context: context.to_owned(),
                keyword: format!("x-satay.{keyword}"),
            }
            .at(self, &extension_location));
        }
        Ok(())
    }

    /// Builds the explicit enum variant names of an enum use.
    ///
    /// # Errors
    ///
    /// Reports enum-incompatible options and unknown wire values, without
    /// assigning or validating Rust names.
    fn enum_variant_names(
        &self,
        options: &SataySchemaOptions,
        effective: &[JsonValue],
        pointer: &str,
        context: &str,
    ) -> Result<Vec<EnumVariantName>, NormalizeError> {
        if let Some(parse_as) = options.parse_as {
            return Err(ValidationError::SatayParseAsWithEnum {
                context: context.to_owned(),
                parse_as: parse_as_name(parse_as).to_owned(),
            }
            .at(self, pointer));
        }
        if options
            .none_if
            .as_ref()
            .is_some_and(|sentinels| sentinels.is_empty())
        {
            return Err(ValidationError::EmptySatayNoneIf {
                context: context.to_owned(),
            }
            .at(self, pointer));
        }
        let unsupported_keyword = options
            .integer_type
            .as_ref()
            .map(|_| "integer-type")
            .or_else(|| options.none_if.as_ref().map(|_| "none-if"))
            .or_else(|| options.true_values.as_ref().map(|_| "true-values"))
            .or_else(|| options.false_values.as_ref().map(|_| "false-values"))
            .or_else(|| options.target.as_ref().map(|_| "target"))
            .or_else(|| options.fields.as_ref().map(|_| "fields"))
            .or_else(|| options.delimiter.as_ref().map(|_| "delimiter"))
            .or_else(|| options.unknown_as.as_ref().map(|_| "unknown-as"));
        if let Some(keyword) = unsupported_keyword {
            return Err(ValidationError::SatayOptionUnsupportedWithEnum {
                context: context.to_owned(),
                keyword,
            }
            .at(self, pointer));
        }

        let Some(mappings) = options.enum_variants.as_ref() else {
            return Ok(vec![]);
        };

        let wire_names = effective
            .iter()
            .filter_map(JsonValue::as_str)
            .collect::<BTreeSet<_>>();

        let mut names = Vec::with_capacity(mappings.len());

        for (wire_value, requested_name) in mappings {
            if !wire_names.contains(wire_value.as_str()) {
                return Err(ValidationError::UnknownSatayEnumVariantValue {
                    context: context.to_owned(),
                    wire_name: wire_value.clone(),
                }
                .at(
                    self,
                    &child_pointer(&child_pointer(pointer, "enum-variants"), wire_value),
                ));
            }
            names.push(EnumVariantName {
                wire_value: wire_value.clone(),
                requested_name: requested_name.clone(),
            });
        }
        Ok(names)
    }

    /// Builds the declared boolean string mapping of a parsed-string bool.
    ///
    /// # Errors
    ///
    /// Reports mappings configured outside a parsed-string bool, incomplete
    /// mappings, empty value lists, and boolean/none-if overlaps at the
    /// `x-satay` extension node.
    fn bool_string_mapping(
        &self,
        options: &SataySchemaOptions,
        none_if: &[String],
        parsed_bool: bool,
        pointer: &str,
        context: &str,
    ) -> Result<Option<BoolMapping>, NormalizeError> {
        let location = pointer;
        let configured = options.true_values.is_some()
            || options.false_values.is_some()
            || options.unknown_as.is_some();
        if !configured {
            return Ok(None);
        }
        if !parsed_bool {
            return Err(ValidationError::SatayBoolMappingRequiresParsedStringBool {
                context: context.to_owned(),
            }
            .at(self, location));
        }
        let (Some(true_values), Some(false_values)) = (
            options.true_values.as_deref(),
            options.false_values.as_deref(),
        ) else {
            return Err(ValidationError::IncompleteSatayBoolMapping {
                context: context.to_owned(),
            }
            .at(self, location));
        };
        let mapping = match BoolMapping::new(
            true_values.to_vec(),
            false_values.to_vec(),
            options.unknown_as,
        ) {
            Ok(mapping) => mapping,
            Err(InterpretationError::EmptyTrueValues) => {
                return Err(ValidationError::EmptySatayBoolMapping {
                    context: context.to_owned(),
                    keyword: "true-values",
                }
                .at(self, location));
            }
            Err(InterpretationError::EmptyFalseValues) => {
                return Err(ValidationError::EmptySatayBoolMapping {
                    context: context.to_owned(),
                    keyword: "false-values",
                }
                .at(self, location));
            }
            Err(InterpretationError::OverlappingBoolValue { value }) => {
                return Err(ValidationError::OverlappingSatayBoolMapping {
                    context: context.to_owned(),
                    value,
                }
                .at(self, location));
            }
            Err(source) => {
                return Err(NormalizeError::Interpretation {
                    location: source_ref(self.document_id, location),
                    source,
                });
            }
        };

        if let Some(value) = none_if.iter().find(|value| {
            mapping.true_values().contains(*value) || mapping.false_values().contains(*value)
        }) {
            return Err(ValidationError::OverlappingSatayBoolMappingNoneIf {
                context: context.to_owned(),
                value: (*value).clone(),
            }
            .at(self, location));
        }

        Ok(Some(mapping))
    }

    /// Derives the property policy of one use from its local options.
    ///
    /// # Errors
    ///
    /// Reports checked sentinel constructor failures at their declaration.
    fn property_policy(
        &self,
        options: &SataySchemaOptions,
        position: SchemaPosition,
        pointer: &str,
    ) -> Result<PropertyPolicy, NormalizeError> {
        if position != SchemaPosition::Property {
            return Ok(PropertyPolicy::default());
        }

        if options.ignore == Some(true) {
            return Ok(PropertyPolicy::Ignored);
        }

        let decoding = if let Some(values) = options.none_if.as_ref() {
            DecodePolicy::SentinelAsAbsent(SentinelValues::new(values.clone()).map_err(
                |source| NormalizeError::Interpretation {
                    location: source_ref(self.document_id, pointer),
                    source,
                },
            )?)
        } else if options.treat_error_as_none == Some(true) {
            DecodePolicy::ErrorAsAbsent
        } else {
            DecodePolicy::PropagateError
        };

        Ok(PropertyPolicy::Included {
            identifier: options
                .identifier
                .as_ref()
                .map(|identifier| identifier.words().to_vec()),
            decoding,
        })
    }

    /// Looks up one component schema by decoded original name.
    ///
    /// Distinct from the schema converter's component lookup: a missing name
    /// is a pointer error located at `location`, while an excluded-but-present
    /// target surfaces at the definitions lookup of the caller.
    fn component_schema_at(
        &self,
        name: &str,
        location: &str,
    ) -> Result<&OasSchema, NormalizeError> {
        self.document
            .spec
            .components
            .as_ref()
            .and_then(|components| components.schemas.get(name))
            .ok_or_else(|| {
                ValidationError::MissingJsonPointerToken {
                    token: name.to_owned(),
                }
                .at(self, location)
            })
    }

    /// Resolves the effective declared enum values of one schema.
    ///
    /// Mirrors the schema converter's classification: a `const` alongside a
    /// non-empty `enum` narrows to the `const` value when it is one of the
    /// declared values and errors otherwise; a non-string `const` under a
    /// non-string type keeps the schema's non-enum handling.
    fn effective_declared_values<'s>(
        schema: &'s OasObjectSchema,
        schema_type: Option<OasSchemaType>,
        context: &str,
    ) -> Result<&'s [JsonValue], ValidationError> {
        let Some(const_value) = schema.const_value.as_ref() else {
            return Ok(&schema.enum_values);
        };

        if !schema.enum_values.is_empty() {
            if schema.enum_values.contains(const_value) {
                return Ok(slice::from_ref(const_value));
            }
            return Err(ValidationError::ConstNotInEnum {
                context: context.to_owned(),
            });
        }

        if const_value.is_string() && matches!(schema_type, Some(OasSchemaType::String) | None) {
            return Ok(slice::from_ref(const_value));
        }

        Ok(&[])
    }
}

/// Maps an explicit `x-satay.integer-type` wire value onto its representation
/// intent.
///
/// `None` keeps no explicit width intent; `auto` stays distinguishable from a
/// selected width.
#[must_use]
pub(in crate::parse) fn integer_representation(
    wire: Option<SatayIntegerTypeWire>,
) -> Option<IntegerRepresentation> {
    let wire = wire?;
    Some(match wire {
        SatayIntegerTypeWire::Auto => IntegerRepresentation::Auto,
        SatayIntegerTypeWire::U8 => IntegerRepresentation::U8,
        SatayIntegerTypeWire::U16 => IntegerRepresentation::U16,
        SatayIntegerTypeWire::U32 => IntegerRepresentation::U32,
        SatayIntegerTypeWire::U64 => IntegerRepresentation::U64,
        SatayIntegerTypeWire::I8 => IntegerRepresentation::I8,
        SatayIntegerTypeWire::I16 => IntegerRepresentation::I16,
        SatayIntegerTypeWire::I32 => IntegerRepresentation::I32,
        SatayIntegerTypeWire::I64 => IntegerRepresentation::I64,
    })
}

/// Maps a scalar `x-satay.parse-as` wire value onto its string scalar 1:1.
///
/// Range and coordinate wire values carry their own interpretation payloads
/// and map to `None`.
#[must_use]
pub(in crate::parse) fn string_scalar_from_wire(wire: SatayParseAsWire) -> Option<StringScalar> {
    Some(match wire {
        SatayParseAsWire::U8 => StringScalar::U8,
        SatayParseAsWire::U16 => StringScalar::U16,
        SatayParseAsWire::U32 => StringScalar::U32,
        SatayParseAsWire::U64 => StringScalar::U64,
        SatayParseAsWire::I8 => StringScalar::I8,
        SatayParseAsWire::I16 => StringScalar::I16,
        SatayParseAsWire::I32 => StringScalar::I32,
        SatayParseAsWire::I64 => StringScalar::I64,
        SatayParseAsWire::F32 => StringScalar::F32,
        SatayParseAsWire::F64 => StringScalar::F64,
        SatayParseAsWire::Bool => StringScalar::Bool,
        SatayParseAsWire::Date => StringScalar::Date,
        SatayParseAsWire::NaiveDatetime => StringScalar::NaiveDatetime,
        SatayParseAsWire::OffsetDatetime => StringScalar::OffsetDatetime,
        SatayParseAsWire::Time => StringScalar::Time,
        SatayParseAsWire::IntegerRange
        | SatayParseAsWire::NumberRange
        | SatayParseAsWire::Coordinates => {
            return None;
        }
    })
}

fn integer_type_name(wire: SatayIntegerTypeWire) -> &'static str {
    match wire {
        SatayIntegerTypeWire::Auto => "auto",
        SatayIntegerTypeWire::U8 => "u8",
        SatayIntegerTypeWire::U16 => "u16",
        SatayIntegerTypeWire::U32 => "u32",
        SatayIntegerTypeWire::U64 => "u64",
        SatayIntegerTypeWire::I8 => "i8",
        SatayIntegerTypeWire::I16 => "i16",
        SatayIntegerTypeWire::I32 => "i32",
        SatayIntegerTypeWire::I64 => "i64",
    }
}

fn parse_as_name(wire: SatayParseAsWire) -> &'static str {
    match wire {
        SatayParseAsWire::U8 => "u8",
        SatayParseAsWire::U16 => "u16",
        SatayParseAsWire::U32 => "u32",
        SatayParseAsWire::U64 => "u64",
        SatayParseAsWire::I8 => "i8",
        SatayParseAsWire::I16 => "i16",
        SatayParseAsWire::I32 => "i32",
        SatayParseAsWire::I64 => "i64",
        SatayParseAsWire::F32 => "f32",
        SatayParseAsWire::F64 => "f64",
        SatayParseAsWire::Bool => "bool",
        SatayParseAsWire::Date => "date",
        SatayParseAsWire::NaiveDatetime => "naive-datetime",
        SatayParseAsWire::OffsetDatetime => "offset-datetime",
        SatayParseAsWire::Time => "time",
        SatayParseAsWire::IntegerRange => "integer-range",
        SatayParseAsWire::NumberRange => "number-range",
        SatayParseAsWire::Coordinates => "coordinates",
    }
}

/// Names the first `x-satay` key a reference cannot carry.
///
/// Mirrors the legacy ref-sibling scan order. Property positions permit the
/// property-local decoding options; everywhere else every key is rejected.
fn reference_satay_keyword(options: &SataySchemaOptions, property: bool) -> Option<&'static str> {
    let keyword = options
        .parse_as
        .as_ref()
        .map(|_| "parse-as")
        .or_else(|| options.target.as_ref().map(|_| "target"))
        .or_else(|| options.fields.as_ref().map(|_| "fields"))
        .or_else(|| options.delimiter.as_ref().map(|_| "delimiter"))
        .or_else(|| options.integer_type.as_ref().map(|_| "integer-type"))
        .or_else(|| options.none_if.as_ref().map(|_| "none-if"))
        .or_else(|| options.true_values.as_ref().map(|_| "true-values"))
        .or_else(|| options.false_values.as_ref().map(|_| "false-values"))
        .or_else(|| options.unknown_as.as_ref().map(|_| "unknown-as"))
        .or_else(|| options.enum_variants.as_ref().map(|_| "enum-variants"));
    if property {
        return keyword;
    }
    keyword
        .or_else(|| {
            options
                .treat_error_as_none
                .as_ref()
                .map(|_| "treat-error-as-none")
        })
        .or_else(|| options.ignore.as_ref().map(|_| "ignore"))
        .or_else(|| options.identifier.as_ref().map(|_| "identifier"))
}

/// One declared target field collected for coordinate validation.
struct DeclaredField<'doc> {
    required: bool,
    schema: &'doc OasSchema,
    pointer: String,
}
