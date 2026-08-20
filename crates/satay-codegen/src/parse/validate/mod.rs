pub(crate) mod constraint;
mod operation;
mod reachability;
mod satay;
mod schema;

use std::mem;

use super::resolve::ResolvedDocument;
use crate::error::ValidationError;
use crate::model::{
    Enum, HttpMethod, IntegerType, ParameterLocation, ParseAs, PathSegment, RangeScalar,
    ResponseStatus, StringCodec, Validation,
};

#[derive(Debug)]
pub(crate) struct ValidatedDocument<'a> {
    pub(crate) resolved: ResolvedDocument<'a>,
    pub(crate) components: Vec<ValidatedComponent>,
    pub(crate) operations: Vec<ValidatedOperation>,
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedComponent {
    pub(crate) schema_name: String,
    pub(crate) description: Option<String>,
    pub(crate) kind: ValidatedComponentKind,
}

#[derive(Debug, Clone)]
pub(crate) enum ValidatedComponentKind {
    Reference(String),
    Struct(Vec<ValidatedField>),
    Type(ValidatedType),
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedField {
    pub(crate) wire_name: String,
    pub(crate) description: Option<String>,
    pub(crate) required: bool,
    pub(crate) value: ValidatedFieldValue,
}

#[derive(Debug, Clone)]
pub(crate) enum ValidatedFieldValue {
    Strict(ValidatedType),
    Lossy(ValidatedType),
    SentinelParsedString {
        ty: ValidatedParsedString,
        sentinels: NonEmptySentinels,
    },
}

impl ValidatedFieldValue {
    pub(crate) fn ty(&self) -> &ValidatedType {
        match self {
            Self::Strict(ty) | Self::Lossy(ty) => ty,
            Self::SentinelParsedString { ty, .. } => ty.as_type(),
        }
    }
}

#[derive(Debug, Clone)]
enum ValidatedFieldDecoding {
    Strict,
    Lossy,
    Sentinel(NonEmptySentinels),
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedType {
    pub(crate) kind: ValidatedTypeKind,
    pub(crate) nullable: bool,
    pub(crate) validation: Option<Validation>,
    pub(crate) description: Option<String>,
    field_decoding: ValidatedFieldDecoding,
    pub(crate) ignore: bool,
    pub(crate) identifier_words: Option<Vec<String>>,
}

impl ValidatedType {
    pub(crate) fn named(rust_name: String) -> Self {
        Self {
            kind: ValidatedTypeKind::Named(rust_name),
            nullable: false,
            validation: None,
            description: None,
            field_decoding: ValidatedFieldDecoding::Strict,
            ignore: false,
            identifier_words: None,
        }
    }

    pub(crate) fn into_field_value(mut self) -> ValidatedFieldValue {
        let decoding = mem::replace(&mut self.field_decoding, ValidatedFieldDecoding::Strict);
        match decoding {
            ValidatedFieldDecoding::Strict => ValidatedFieldValue::Strict(self),
            ValidatedFieldDecoding::Lossy => ValidatedFieldValue::Lossy(self),
            ValidatedFieldDecoding::Sentinel(sentinels) => {
                let Ok(ty) = ValidatedParsedString::try_from_type(self) else {
                    unreachable!("sentinel decoding is validated only for parsed strings")
                };
                ValidatedFieldValue::SentinelParsedString { ty, sentinels }
            }
        }
    }

    pub(crate) fn is_nullable(&self) -> bool {
        self.nullable
    }

    pub(crate) fn is_array(&self) -> bool {
        matches!(self.kind, ValidatedTypeKind::Array(_))
    }

    pub(crate) fn contains_any_of(&self) -> bool {
        match &self.kind {
            ValidatedTypeKind::AnyOf(_) => true,
            ValidatedTypeKind::Array(item) | ValidatedTypeKind::Map(item) => item.contains_any_of(),
            ValidatedTypeKind::InlineStruct(fields) => fields
                .iter()
                .any(|field| field.value.ty().contains_any_of()),
            ValidatedTypeKind::Named(_)
            | ValidatedTypeKind::String
            | ValidatedTypeKind::ParsedString(_)
            | ValidatedTypeKind::ParsedInteger(_)
            | ValidatedTypeKind::Integer(_)
            | ValidatedTypeKind::F32
            | ValidatedTypeKind::F64
            | ValidatedTypeKind::Bool
            | ValidatedTypeKind::JsonValue
            | ValidatedTypeKind::Enum(_)
            | ValidatedTypeKind::Range(_) => false,
        }
    }

    pub(crate) fn contains_map_or_json_value(&self) -> bool {
        match &self.kind {
            ValidatedTypeKind::Map(_) | ValidatedTypeKind::JsonValue => true,
            ValidatedTypeKind::Array(item) => item.contains_map_or_json_value(),
            ValidatedTypeKind::InlineStruct(fields) => fields
                .iter()
                .any(|field| field.value.ty().contains_map_or_json_value()),
            ValidatedTypeKind::AnyOf(union) => {
                union.variants.iter().any(|variant| match &variant.kind {
                    ValidatedUnionVariantKind::Reference { .. } => false,
                    ValidatedUnionVariantKind::Inline(ty) => ty.contains_map_or_json_value(),
                })
            }
            ValidatedTypeKind::Named(_)
            | ValidatedTypeKind::String
            | ValidatedTypeKind::ParsedString(_)
            | ValidatedTypeKind::ParsedInteger(_)
            | ValidatedTypeKind::Integer(_)
            | ValidatedTypeKind::F32
            | ValidatedTypeKind::F64
            | ValidatedTypeKind::Bool
            | ValidatedTypeKind::Enum(_)
            | ValidatedTypeKind::Range(_) => false,
        }
    }

    pub(crate) fn contains_inline_struct(&self) -> bool {
        match &self.kind {
            ValidatedTypeKind::InlineStruct(_) => true,
            ValidatedTypeKind::Array(item) | ValidatedTypeKind::Map(item) => {
                item.contains_inline_struct()
            }
            ValidatedTypeKind::AnyOf(union) => {
                union.variants.iter().any(|variant| match &variant.kind {
                    ValidatedUnionVariantKind::Reference { .. } => false,
                    ValidatedUnionVariantKind::Inline(ty) => ty.contains_inline_struct(),
                })
            }
            ValidatedTypeKind::Named(_)
            | ValidatedTypeKind::String
            | ValidatedTypeKind::ParsedString(_)
            | ValidatedTypeKind::ParsedInteger(_)
            | ValidatedTypeKind::Integer(_)
            | ValidatedTypeKind::F32
            | ValidatedTypeKind::F64
            | ValidatedTypeKind::Bool
            | ValidatedTypeKind::JsonValue
            | ValidatedTypeKind::Enum(_)
            | ValidatedTypeKind::Range(_) => false,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ValidatedTypeKind {
    Named(String),
    String,
    ParsedString(StringCodec),
    ParsedInteger(ParseAs),
    Integer(IntegerType),
    F32,
    F64,
    Bool,
    Array(Box<ValidatedType>),
    /// A JSON object with arbitrary keys and a uniform value schema.
    Map(Box<ValidatedType>),
    /// Any JSON value (an empty JSON schema accepts everything).
    JsonValue,
    Enum(Enum),
    AnyOf(ValidatedUnion),
    InlineStruct(Vec<ValidatedField>),
    Range(RangeScalar),
}

/// An ordered, non-empty list of wire strings that decode to a single value.
#[derive(Debug, Clone)]
pub(crate) struct NonEmptySentinels {
    values: Box<[String]>,
}

/// Error returned when a [`NonEmptySentinels`] constructor receives an empty list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmptySentinels {
    Empty,
}

impl NonEmptySentinels {
    pub(crate) fn new(values: Vec<String>) -> Result<Self, EmptySentinels> {
        if values.is_empty() {
            return Err(EmptySentinels::Empty);
        }
        Ok(Self {
            values: values.into_boxed_slice(),
        })
    }

    pub(crate) fn as_slice(&self) -> &[String] {
        &self.values
    }
}

/// A validated string decoded via a [`StringCodec`].
///
/// The private wrapper can only be constructed from a parsed-string kind.
#[derive(Debug, Clone)]
pub(crate) struct ValidatedParsedString {
    ty: ValidatedType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NotParsedString;

impl ValidatedParsedString {
    #[cfg(test)]
    pub(crate) fn new(codec: StringCodec, nullable: bool, description: Option<String>) -> Self {
        Self {
            ty: ValidatedType {
                kind: ValidatedTypeKind::ParsedString(codec),
                nullable,
                validation: None,
                description,
                field_decoding: ValidatedFieldDecoding::Strict,
                ignore: false,
                identifier_words: None,
            },
        }
    }

    fn try_from_type(ty: ValidatedType) -> Result<Self, NotParsedString> {
        if matches!(ty.kind, ValidatedTypeKind::ParsedString(_)) {
            Ok(Self { ty })
        } else {
            Err(NotParsedString)
        }
    }

    pub(crate) fn as_type(&self) -> &ValidatedType {
        &self.ty
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedUnion {
    pub(crate) variants: Vec<ValidatedUnionVariant>,
    pub(crate) tag: Option<ValidatedUnionTag>,
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedUnionTag {
    pub(crate) property_name: String,
    pub(crate) style: ValidatedUnionTagStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidatedUnionTagStyle {
    InternallyTagged,
    EmbeddedField,
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedUnionVariant {
    pub(crate) rust_name: String,
    pub(crate) kind: ValidatedUnionVariantKind,
    pub(crate) tag_value: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum ValidatedUnionVariantKind {
    Reference {
        type_name: String,
        schema_name: String,
    },
    Inline(ValidatedType),
}

#[derive(Debug)]
pub(crate) struct ValidatedOperation {
    pub(crate) operation_id: String,
    pub(crate) tags: Vec<String>,
    pub(crate) description: Option<String>,
    pub(crate) method: HttpMethod,
    pub(crate) path: String,
    pub(crate) path_segments: Vec<PathSegment>,
    pub(crate) parameters: Vec<ValidatedParameter>,
    pub(crate) request_body: Option<ValidatedRequestBody>,
    pub(crate) responses: Vec<ValidatedResponse>,
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedParameter {
    pub(crate) location: ParameterLocation,
    pub(crate) wire_name: String,
    pub(crate) description: Option<String>,
    pub(crate) ty: ValidatedType,
    pub(crate) required: bool,
}

#[derive(Debug)]
pub(crate) struct ValidatedRequestBody {
    pub(crate) description: Option<String>,
    pub(crate) content_type: String,
    pub(crate) ty: ValidatedType,
    pub(crate) required: bool,
}

#[derive(Debug)]
pub(crate) struct ValidatedResponse {
    pub(crate) status: ResponseStatus,
    pub(crate) description: Option<String>,
    pub(crate) body: Option<ValidatedType>,
    pub(crate) projection: Option<ValidatedResponseProjection>,
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedResponseProjection {
    pub(crate) unwrap_field: String,
    pub(crate) map_field: Option<String>,
}

pub(crate) fn validate_document<'a>(
    document: ResolvedDocument<'a>,
) -> Result<ValidatedDocument<'a>, ValidationError> {
    let openapi = document.spec.openapi.as_str();

    if !is_supported_openapi_version(openapi) {
        return Err(ValidationError::UnsupportedOpenApiVersion {
            version: openapi.to_owned(),
        });
    }

    let excluded = reachability::excluded_component_schemas(&document)?;
    let components = schema::validate_components(&document, &excluded)?;
    let operations = operation::validate_operations(&document)?;

    Ok(ValidatedDocument {
        resolved: document,
        components,
        operations,
    })
}

fn is_supported_openapi_version(version: &str) -> bool {
    version.starts_with("3.1.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_sentinels_rejects_empty_input() {
        let err = NonEmptySentinels::new(vec![]).unwrap_err();
        assert!(matches!(err, EmptySentinels::Empty));
    }

    #[test]
    fn non_empty_sentinels_retains_input_order() {
        let sentinels = NonEmptySentinels::new(vec![
            "n/a".to_owned(),
            "null".to_owned(),
            "missing".to_owned(),
        ])
        .expect("non-empty input is accepted");

        assert_eq!(sentinels.as_slice(), &["n/a", "null", "missing"]);
    }

    #[test]
    fn parsed_string_construction_sets_kind_and_defaults() {
        let parsed = ValidatedParsedString::new(
            StringCodec::Standard(ParseAs::OffsetDateTime),
            true,
            Some("creation timestamp".to_owned()),
        );

        let ty = parsed.as_type();
        assert!(matches!(
            ty.kind,
            ValidatedTypeKind::ParsedString(StringCodec::Standard(ParseAs::OffsetDateTime))
        ));
        assert!(ty.nullable);
        assert_eq!(ty.description.as_deref(), Some("creation timestamp"));
        assert!(ty.validation.is_none());
        assert!(matches!(ty.field_decoding, ValidatedFieldDecoding::Strict));
        assert!(!ty.ignore);
        assert!(ty.identifier_words.is_none());
    }

    #[test]
    fn field_values_encode_exclusive_decoding_modes() {
        let strict = ValidatedType::named("StrictValue".to_owned()).into_field_value();
        assert!(matches!(strict, ValidatedFieldValue::Strict(_)));

        let mut lossy_ty = ValidatedType::named("LossyValue".to_owned());
        lossy_ty.field_decoding = ValidatedFieldDecoding::Lossy;
        let lossy = lossy_ty.into_field_value();
        assert!(matches!(lossy, ValidatedFieldValue::Lossy(_)));

        let mut sentinel_ty =
            ValidatedParsedString::new(StringCodec::Standard(ParseAs::F64), false, None).ty;
        sentinel_ty.field_decoding = ValidatedFieldDecoding::Sentinel(
            NonEmptySentinels::new(vec!["NA".to_owned()]).unwrap(),
        );
        let sentinel = sentinel_ty.into_field_value();
        assert!(matches!(
            sentinel,
            ValidatedFieldValue::SentinelParsedString { .. }
        ));
    }

    #[test]
    fn parsed_string_proof_rejects_other_types() {
        let ty = ValidatedType::named("NotParsed".to_owned());
        assert!(ValidatedParsedString::try_from_type(ty).is_err());
    }
}
