//! OpenAPI syntax checks used before constructing the semantic graph.
use crate::error::ValidationError;
use crate::model::HttpMethod;
use crate::parse::satay::{SatayIdentifier, SataySchemaOptions, operation_options};
use crate::parse::{helpers, reference::schema_type_wire};
use oas3::spec::{
    ObjectSchema as OasObjectSchema, Operation as OasOperation, Schema as OasSchema,
    SchemaType as OasSchemaType, SchemaTypeSet as OasSchemaTypeSet,
};
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
/// Annotation keywords permitted beside a single `allOf`/`$ref` branch.
const ALLOWED_ANNOTATION_KEYWORDS: &[&str] = &[
    "title",
    "description",
    "default",
    "deprecated",
    "readOnly",
    "writeOnly",
    "examples",
    "example",
];

pub(in crate::parse) fn unsupported_reference_schema_keyword(
    schema: &OasObjectSchema,
) -> Option<&str> {
    schema
        .present_keywords()
        .find(|&keyword| keyword != "$ref" && keyword != "description")
}

pub(in crate::parse) fn reject_preserved_unknown_keywords(
    schema: &OasSchema,
    context: &str,
) -> Result<(), ValidationError> {
    if let Some(object) = schema.as_object()
        && let Some(keyword) = object
            .present_keywords()
            .find(|keyword| object.unknown_keywords.contains_key(*keyword))
    {
        return Err(ValidationError::UnsupportedKeyword {
            context: context.to_owned(),
            keyword: keyword.to_owned(),
        });
    }

    for subschema in schema.subschemas() {
        reject_preserved_unknown_keywords(subschema, context)?;
    }

    Ok(())
}

pub(in crate::parse) fn annotation_only_all_of_ref_wrapper(
    schema: &OasObjectSchema,
) -> Option<&str> {
    if schema.all_of.len() != 1 {
        return None;
    }
    let reference = schema.all_of[0].reference()?;

    let only_annotations = schema
        .present_keywords()
        .all(|keyword| keyword == "allOf" || ALLOWED_ANNOTATION_KEYWORDS.contains(&keyword));
    let no_semantic_extension = unsupported_union_extension(schema).is_none();

    (only_annotations && no_semantic_extension).then_some(reference)
}

pub(in crate::parse) fn reject_all_of_sibling_keywords(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    if !composite_object_type_is_allowed(schema) {
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

pub(in crate::parse) fn reject_all_of_object_branch_keywords(
    schema: &OasObjectSchema,
    context: &str,
    index: usize,
) -> Result<(), ValidationError> {
    if !composite_object_type_is_allowed(schema) {
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
            schema.additional_properties.is_some() && schema.properties.is_empty(),
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

fn composite_object_type_is_allowed(schema: &OasObjectSchema) -> bool {
    matches!(
        schema.schema_type.as_ref(),
        None | Some(OasSchemaTypeSet::Single(OasSchemaType::Object))
    )
}

pub(in crate::parse) fn reject_any_of_sibling_keywords(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    reject_plain_union_sibling_keywords(schema, context, PlainUnionKeyword::AnyOf)
}

pub(in crate::parse) fn reject_plain_one_of_sibling_keywords(
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

pub(in crate::parse) fn reject_discriminator_union_sibling_keywords(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    if !composite_object_type_is_allowed(schema) {
        return Err(ValidationError::UnsupportedAnyOfSiblingKeyword {
            context: context.to_owned(),
            keyword: "type".to_owned(),
        });
    }

    for (keyword, present) in [
        ("allOf", !schema.all_of.is_empty()),
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

pub(in crate::parse) fn validate_enum_shape(
    enum_values: &[JsonValue],
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

    if enum_values.is_empty() {
        return Err(ValidationError::EmptyEnum {
            context: context.to_owned(),
        });
    }

    for value in enum_values {
        if value.as_str().is_none() {
            return Err(ValidationError::NonStringEnumValue {
                context: context.to_owned(),
            });
        }
    }

    Ok(())
}

pub(super) fn operation_satay_skip(
    operation: &OasOperation,
    operation_id: &str,
) -> Result<bool, ValidationError> {
    let context = format!("operation `{operation_id}`");
    let options = operation_options(operation, &context)?.unwrap_or_default();
    Ok(options.skip)
}

pub(in crate::parse) fn wildcard_status_class(status: &str) -> Option<u8> {
    match status.as_bytes() {
        [class @ b'1'..=b'5', b'X', b'X'] => Some(class - b'0'),
        _ => None,
    }
}

pub(in crate::parse) fn path_parameter_names(
    path: &str,
) -> Result<BTreeSet<String>, ValidationError> {
    let mut names = BTreeSet::new();
    let mut rest = path;

    loop {
        let Some(open) = rest.find('{') else {
            return Ok(names);
        };

        let close = rest[open + 1..].find('}').ok_or_else(|| {
            let path = path.to_owned();
            ValidationError::UnclosedPathParameter { path }
        })?;

        names.insert(rest[open + 1..open + 1 + close].to_owned());
        rest = &rest[open + 1 + close + 1..];
    }
}

pub(super) fn inferred_operation_id(method: HttpMethod, path: &str) -> String {
    helpers::inferred_operation_id(method.operation_prefix(), path)
}

pub(in crate::parse) fn reject_property_options_on_value(
    treat_error_as_none: Option<bool>,
    none_if: Option<&[String]>,
    ignore: Option<bool>,
    identifier: Option<&SatayIdentifier>,
    context: &str,
) -> Result<(), ValidationError> {
    if treat_error_as_none.is_some() {
        return Err(
            ValidationError::SatayTreatErrorAsNoneRequiresObjectProperty {
                context: context.to_owned(),
            },
        );
    }
    if none_if.is_some() {
        return Err(ValidationError::SatayNoneIfRequiresStructField {
            context: context.to_owned(),
        });
    }
    if ignore.is_some() {
        return Err(ValidationError::SatayIgnoreRequiresObjectProperty {
            context: context.to_owned(),
        });
    }
    if identifier.is_some() {
        return Err(ValidationError::SatayIdentifierRequiresObjectProperty {
            context: context.to_owned(),
        });
    }

    Ok(())
}

pub(in crate::parse) fn reject_options_with_ignore(
    options: &SataySchemaOptions,
    context: &str,
) -> Result<(), ValidationError> {
    if options.ignore != Some(true) {
        return Ok(());
    }

    let conflicting_keyword = options
        .treat_error_as_none
        .as_ref()
        .map(|_| "treat-error-as-none")
        .or_else(|| options.none_if.as_ref().map(|_| "none-if"))
        .or_else(|| options.true_values.as_ref().map(|_| "true-values"))
        .or_else(|| options.false_values.as_ref().map(|_| "false-values"))
        .or_else(|| options.unknown_as.as_ref().map(|_| "unknown-as"))
        .or_else(|| options.enum_variants.as_ref().map(|_| "enum-variants"))
        .or_else(|| options.parse_as.as_ref().map(|_| "parse-as"))
        .or_else(|| options.integer_type.as_ref().map(|_| "integer-type"))
        .or_else(|| options.target.as_ref().map(|_| "target"))
        .or_else(|| options.fields.as_ref().map(|_| "fields"))
        .or_else(|| options.delimiter.as_ref().map(|_| "delimiter"))
        .or_else(|| options.identifier.as_ref().map(|_| "identifier"));
    if let Some(keyword) = conflicting_keyword {
        return Err(ValidationError::SatayOptionConflictsWithIgnore {
            context: context.to_owned(),
            keyword,
        });
    }

    Ok(())
}

pub(super) fn is_supported_openapi_version(version: &str) -> bool {
    version.starts_with("3.1.")
}
