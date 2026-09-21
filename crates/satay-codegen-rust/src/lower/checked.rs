//! Rust-owned checked representations between semantic lowering and rendering.
use crate::model::{
    CoordinateDelimiter, Enum, HttpMethod, IntegerType, ParameterDefault, ParameterLocation,
    ParseAs, PathSegment, RangeScalar, ResponseStatus, StringCodec, Validation,
};

#[derive(Debug, Clone)]
pub(crate) struct CheckedComponent {
    pub(crate) schema_name: String,
    pub(crate) description: Option<String>,
    pub(crate) kind: CheckedComponentKind,
}

#[derive(Debug, Clone)]
pub(crate) enum CheckedComponentKind {
    Reference(String),
    Struct(Vec<CheckedField>),
    Type(CheckedType),
}

#[derive(Debug, Clone)]
pub(crate) struct CheckedField {
    pub(crate) wire_name: String,
    pub(crate) description: Option<String>,
    pub(crate) identifier: Option<Vec<String>>,
    pub(crate) required: bool,
    pub(crate) value: CheckedFieldValue,
}

#[derive(Debug, Clone)]
pub(crate) enum CheckedFieldValue {
    Strict(CheckedType),
    Lossy(CheckedType),
    SentinelParsedString {
        ty: CheckedParsedString,
        sentinels: NonEmptySentinels,
    },
}

impl CheckedFieldValue {
    pub(crate) fn ty(&self) -> &CheckedType {
        match self {
            Self::Strict(ty) | Self::Lossy(ty) => ty,
            Self::SentinelParsedString { ty, .. } => ty.as_type(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CheckedType {
    pub(crate) kind: CheckedTypeKind,
    pub(crate) nullable: bool,
    pub(crate) validation: Option<Validation>,
    pub(crate) description: Option<String>,
}

impl CheckedType {
    pub(crate) fn named(rust_name: String) -> Self {
        Self {
            kind: CheckedTypeKind::Named(rust_name),
            nullable: false,
            validation: None,
            description: None,
        }
    }

    pub(crate) fn is_array(&self) -> bool {
        matches!(self.kind, CheckedTypeKind::Array(_))
    }

    pub(crate) fn contains_any_of(&self) -> bool {
        match &self.kind {
            CheckedTypeKind::AnyOf(_) => true,
            CheckedTypeKind::Array(item) | CheckedTypeKind::Map(item) => item.contains_any_of(),
            CheckedTypeKind::InlineStruct(fields) => fields
                .iter()
                .any(|field| field.value.ty().contains_any_of()),
            CheckedTypeKind::Named(_)
            | CheckedTypeKind::String
            | CheckedTypeKind::ParsedString(_)
            | CheckedTypeKind::Coordinates(_)
            | CheckedTypeKind::ParsedInteger(_)
            | CheckedTypeKind::Integer(_)
            | CheckedTypeKind::F32
            | CheckedTypeKind::F64
            | CheckedTypeKind::Bool
            | CheckedTypeKind::JsonValue
            | CheckedTypeKind::Enum(_)
            | CheckedTypeKind::Range(_) => false,
        }
    }

    pub(crate) fn contains_map_or_json_value(&self) -> bool {
        match &self.kind {
            CheckedTypeKind::Map(_) | CheckedTypeKind::JsonValue => true,
            CheckedTypeKind::Array(item) => item.contains_map_or_json_value(),
            CheckedTypeKind::InlineStruct(fields) => fields
                .iter()
                .any(|field| field.value.ty().contains_map_or_json_value()),
            CheckedTypeKind::AnyOf(union) => {
                union.variants.iter().any(|variant| match &variant.kind {
                    CheckedUnionVariantKind::Reference { .. } => false,
                    CheckedUnionVariantKind::Inline(ty) => ty.contains_map_or_json_value(),
                })
            }
            CheckedTypeKind::Named(_)
            | CheckedTypeKind::String
            | CheckedTypeKind::ParsedString(_)
            | CheckedTypeKind::Coordinates(_)
            | CheckedTypeKind::ParsedInteger(_)
            | CheckedTypeKind::Integer(_)
            | CheckedTypeKind::F32
            | CheckedTypeKind::F64
            | CheckedTypeKind::Bool
            | CheckedTypeKind::Enum(_)
            | CheckedTypeKind::Range(_) => false,
        }
    }

    pub(crate) fn contains_inline_struct(&self) -> bool {
        match &self.kind {
            CheckedTypeKind::InlineStruct(_) => true,
            CheckedTypeKind::Array(item) | CheckedTypeKind::Map(item) => {
                item.contains_inline_struct()
            }
            CheckedTypeKind::AnyOf(union) => {
                union.variants.iter().any(|variant| match &variant.kind {
                    CheckedUnionVariantKind::Reference { .. } => false,
                    CheckedUnionVariantKind::Inline(ty) => ty.contains_inline_struct(),
                })
            }
            CheckedTypeKind::Named(_)
            | CheckedTypeKind::String
            | CheckedTypeKind::ParsedString(_)
            | CheckedTypeKind::Coordinates(_)
            | CheckedTypeKind::ParsedInteger(_)
            | CheckedTypeKind::Integer(_)
            | CheckedTypeKind::F32
            | CheckedTypeKind::F64
            | CheckedTypeKind::Bool
            | CheckedTypeKind::JsonValue
            | CheckedTypeKind::Enum(_)
            | CheckedTypeKind::Range(_) => false,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum CheckedTypeKind {
    Named(String),
    String,
    ParsedString(StringCodec),
    Coordinates(CheckedCoordinates),
    ParsedInteger(ParseAs),
    Integer(IntegerType),
    F32,
    F64,
    Bool,
    Array(Box<CheckedType>),
    /// A JSON object with arbitrary keys and a uniform value schema.
    Map(Box<CheckedType>),
    /// Any JSON value (an empty JSON schema accepts everything).
    JsonValue,
    Enum(Enum),
    AnyOf(CheckedUnion),
    InlineStruct(Vec<CheckedField>),
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

/// A validated string decoded via a scalar or coordinate field codec.
///
/// The private wrapper can only be constructed from a string-codec kind.
#[derive(Debug, Clone)]
pub(crate) struct CheckedParsedString {
    ty: CheckedType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotParsedString;

impl CheckedParsedString {
    pub(crate) fn try_from_type(ty: CheckedType) -> Result<Self, NotParsedString> {
        if matches!(
            ty.kind,
            CheckedTypeKind::ParsedString(_) | CheckedTypeKind::Coordinates(_)
        ) {
            Ok(Self { ty })
        } else {
            Err(NotParsedString)
        }
    }

    pub(crate) fn as_type(&self) -> &CheckedType {
        &self.ty
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CheckedUnion {
    pub(crate) variants: Vec<CheckedUnionVariant>,
    pub(crate) tag: Option<CheckedUnionTag>,
}

#[derive(Debug, Clone)]
pub(crate) struct CheckedUnionTag {
    pub(crate) property_name: String,
    pub(crate) style: CheckedUnionTagStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckedUnionTagStyle {
    InternallyTagged,
    EmbeddedField,
}

#[derive(Debug, Clone)]
pub(crate) struct CheckedUnionVariant {
    pub(crate) rust_name: String,
    pub(crate) kind: CheckedUnionVariantKind,
    pub(crate) tag_value: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum CheckedUnionVariantKind {
    Reference {
        type_name: String,
        schema_name: String,
    },
    Inline(CheckedType),
}

#[derive(Debug)]
pub(crate) struct CheckedOperation {
    pub(crate) operation_id: String,
    pub(crate) tags: Vec<String>,
    pub(crate) description: Option<String>,
    pub(crate) method: HttpMethod,
    pub(crate) path: String,
    pub(crate) path_segments: Vec<PathSegment>,
    pub(crate) parameters: Vec<CheckedParameter>,
    pub(crate) request_body: Option<CheckedRequestBody>,
    pub(crate) responses: Vec<CheckedResponse>,
}

#[derive(Debug, Clone)]
pub(crate) struct CheckedParameter {
    pub(crate) location: ParameterLocation,
    pub(crate) wire_name: String,
    pub(crate) description: Option<String>,
    pub(crate) ty: CheckedType,
    pub(crate) required: bool,
    pub(crate) default: Option<ParameterDefault>,
}

#[derive(Debug)]
pub(crate) struct CheckedRequestBody {
    pub(crate) description: Option<String>,
    pub(crate) content_type: String,
    pub(crate) ty: CheckedType,
    pub(crate) required: bool,
}

#[derive(Debug)]
pub(crate) struct CheckedResponse {
    pub(crate) status: ResponseStatus,
    pub(crate) description: Option<String>,
    pub(crate) body: Option<CheckedType>,
    pub(crate) projection: Option<CheckedResponseProjection>,
}

#[derive(Debug, Clone)]
pub(crate) struct CheckedResponseProjection {
    pub(crate) unwrap_field: String,
    pub(crate) map_field: Option<String>,
}

/// A resolved generated object with two distinct, required, nonnullable float fields.
/// Rust validation constructs this proof after resolving the semantic selector.
#[derive(Debug, Clone)]
pub(crate) struct CheckedCoordinates {
    target: String,
    field_indices: [usize; 2],
    delimiter: CoordinateDelimiter,
}

impl CheckedCoordinates {
    pub(crate) fn from_semantic(
        target: String,
        field_indices: [usize; 2],
        delimiter: CoordinateDelimiter,
    ) -> Self {
        Self {
            target,
            field_indices,
            delimiter,
        }
    }
    pub(crate) fn target(&self) -> &str {
        &self.target
    }

    pub(crate) fn field_indices(&self) -> [usize; 2] {
        self.field_indices
    }

    pub(crate) fn delimiter(&self) -> &CoordinateDelimiter {
        &self.delimiter
    }
}
