use std::{
    collections::BTreeSet,
    fmt::{self, Display},
};

#[derive(Debug)]
pub(crate) struct Api {
    pub(crate) server_url: String,
    pub(crate) api_key_security_schemes: Vec<ApiKeySecurityScheme>,
    pub(crate) components: Vec<Component>,
    pub(crate) constrained_types: Vec<ConstrainedType>,
    pub(crate) groups: Vec<ApiGroup>,
    pub(crate) operations: Vec<Operation>,
}

impl Api {
    pub(crate) fn new(
        server_url: String,
        api_key_security_schemes: Vec<ApiKeySecurityScheme>,
        components: Vec<Component>,
        constrained_types: Vec<ConstrainedType>,
        groups: Vec<ApiGroup>,
        operations: Vec<Operation>,
    ) -> Self {
        Self {
            server_url,
            api_key_security_schemes,
            components,
            constrained_types,
            groups,
            operations,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ApiGroup {
    /// Original OpenAPI tag. `None` identifies the generated untagged group.
    pub(crate) wire_name: Option<String>,
    pub(crate) rust_name: String,
    pub(crate) description: Option<String>,
    pub(crate) operations: Vec<GroupOperation>,
}

#[derive(Debug)]
pub(crate) struct GroupOperation {
    pub(crate) operation_index: usize,
    pub(crate) method_name: String,
}

#[derive(Debug)]
pub(crate) struct ApiKeySecurityScheme {
    pub(crate) location: ApiKeyLocation,
    pub(crate) wire_name: String,
    pub(crate) rust_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApiKeyLocation {
    Header,
    Query,
}

#[derive(Debug, Clone)]
pub(crate) struct Component {
    pub(crate) rust_name: String,
    pub(crate) description: Option<String>,
    pub(crate) kind: ComponentKind,
}

#[derive(Debug, Clone)]
pub(crate) enum ComponentKind {
    Struct(Vec<Field>),
    Enum(Enum),
    Union(Union),
    Range(RangeType),
    Alias(TypeRef),
    Nutype(ConstrainedType),
}

#[derive(Debug, Clone)]
pub(crate) struct RangeType {
    pub(crate) rust_name: String,
    pub(crate) description: Option<String>,
    pub(crate) scalar: RangeScalar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RangeTypeRef {
    pub(crate) rust_name: String,
    pub(crate) scalar: RangeScalar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeScalar {
    Integer(IntegerType),
    F32,
    F64,
}

#[derive(Debug, Clone)]
pub(crate) struct ConstrainedType {
    pub(crate) rust_name: String,
    pub(crate) description: Option<String>,
    pub(crate) inner: TypeRef,
    pub(crate) validation: Validation,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Validation {
    String {
        min_length: Option<u64>,
        max_length: Option<u64>,
        pattern: Option<String>,
    },
    Integer {
        minimum: Option<IntegerLimit>,
        maximum: Option<IntegerLimit>,
    },
    Number {
        minimum: Option<FloatLimit>,
        maximum: Option<FloatLimit>,
    },
    Array {
        min_items: Option<u64>,
        max_items: Option<u64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IntegerLimit {
    pub(crate) value: i128,
    pub(crate) exclusive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntegerType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloatLimit {
    pub(crate) value: f64,
    pub(crate) exclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoolStringMapping {
    true_values: Vec<String>,
    false_values: Vec<String>,
    unknown_as: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoolStringMappingError {
    EmptyTrueValues,
    EmptyFalseValues,
    OverlappingValue(String),
}

impl BoolStringMapping {
    pub(crate) fn try_new(
        true_values: Vec<String>,
        false_values: Vec<String>,
        unknown_as: Option<bool>,
    ) -> Result<Self, BoolStringMappingError> {
        if true_values.is_empty() {
            return Err(BoolStringMappingError::EmptyTrueValues);
        }
        if false_values.is_empty() {
            return Err(BoolStringMappingError::EmptyFalseValues);
        }
        let true_value_set = true_values
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();

        if let Some(value) = false_values
            .iter()
            .find(|value| true_value_set.contains(value.as_str()))
        {
            return Err(BoolStringMappingError::OverlappingValue(value.clone()));
        }

        Ok(Self {
            true_values,
            false_values,
            unknown_as,
        })
    }

    pub(crate) fn true_values(&self) -> &[String] {
        &self.true_values
    }

    pub(crate) fn false_values(&self) -> &[String] {
        &self.false_values
    }

    pub(crate) fn unknown_as(&self) -> Option<bool> {
        self.unknown_as
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StringCodec {
    Standard(ParseAs),
    MappedBool(BoolStringMapping),
}

impl StringCodec {
    pub(crate) fn parse_as(&self) -> ParseAs {
        match self {
            Self::Standard(parse_as) => *parse_as,
            Self::MappedBool(_) => ParseAs::Bool,
        }
    }

    pub(crate) fn bool_string_mapping(&self) -> Option<&BoolStringMapping> {
        match self {
            Self::MappedBool(mapping) => Some(mapping),
            Self::Standard(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Field {
    pub(crate) wire_name: String,
    pub(crate) identifier_words: Option<Vec<String>>,
    pub(crate) rust_name: String,
    pub(crate) description: Option<String>,
    pub(crate) ty: TypeRef,
    pub(crate) required: bool,
    pub(crate) treat_error_as_none: bool,
    pub(crate) none_if: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Enum {
    pub(crate) variants: Vec<EnumVariant>,
    pub(crate) fallback: EnumFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnumFallback {
    None,
    OtherString,
}

#[derive(Debug, Clone)]
pub(crate) struct EnumVariant {
    pub(crate) wire_name: String,
    pub(crate) rust_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Union {
    pub(crate) variants: Vec<UnionVariant>,
    pub(crate) tag: Option<UnionTag>,
}

#[derive(Debug, Clone)]
pub(crate) struct UnionTag {
    pub(crate) property_name: String,
    pub(crate) style: UnionTagStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnionTagStyle {
    InternallyTagged,
    EmbeddedField,
}

#[derive(Debug, Clone)]
pub(crate) struct UnionVariant {
    pub(crate) rust_name: String,
    pub(crate) ty: TypeRef,
    pub(crate) tag_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TypeRef {
    String,
    ParsedString(StringCodec),
    ParsedInteger(ParseAs),
    Integer(IntegerType),
    F32,
    F64,
    Bool,
    Array(Box<TypeRef>),
    /// A JSON object with arbitrary keys, rendered as `BTreeMap<String, V>`.
    Map(Box<TypeRef>),
    /// Any JSON value, rendered as `satay_runtime::JsonValue`.
    JsonValue,
    Range(RangeTypeRef),
    Named(String),
    Constrained {
        rust_name: String,
        inner: Box<TypeRef>,
    },
    Option(Box<TypeRef>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParseAs {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Bool,
    Date,
    NaiveDateTime,
    OffsetDateTime,
    UnixTime,
    Time,
    IntegerRange,
    NumberRange,
}

#[derive(Debug)]
pub(crate) struct Operation {
    pub(crate) fn_name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) description: Option<String>,
    pub(crate) input_name: String,
    pub(crate) response_name: String,
    pub(crate) method: HttpMethod,
    pub(crate) path: String,
    pub(crate) path_segments: Vec<PathSegment>,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) request_body: Option<RequestBody>,
    pub(crate) responses: Vec<ResponseCase>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HttpMethod {
    Delete,
    Get,
    Head,
    Options,
    Patch,
    Post,
    Put,
    Trace,
}

#[derive(Debug, Clone)]
pub(crate) enum PathSegment {
    Literal(String),
    Parameter(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Parameter {
    pub(crate) location: ParameterLocation,
    pub(crate) wire_name: String,
    pub(crate) rust_name: String,
    pub(crate) description: Option<String>,
    pub(crate) ty: TypeRef,
    pub(crate) required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParameterLocation {
    Path,
    Query,
    Header,
}

#[derive(Debug)]
pub(crate) struct RequestBody {
    pub(crate) field_name: String,
    pub(crate) description: Option<String>,
    pub(crate) content_type: String,
    pub(crate) ty: TypeRef,
    pub(crate) required: bool,
}

/// A response key: an exact status code or an OpenAPI `NXX` wildcard range.
///
/// `Exact` sorts before `Range` so explicit codes shadow the range covering
/// them in generated decode match arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ResponseStatus {
    Exact(u16),
    /// The N of `NXX`, 1..=5.
    Range(u8),
}

impl Display for ResponseStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(code) => write!(f, "{code}"),
            Self::Range(class) => write!(f, "{class}XX"),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ResponseCase {
    pub(crate) status: ResponseStatus,
    pub(crate) variant_name: String,
    pub(crate) description: Option<String>,
    pub(crate) body: Option<TypeRef>,
    pub(crate) projection: Option<ResponseProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResponseProjection {
    pub(crate) unwrap_field: String,
    pub(crate) map_field: Option<String>,
}

impl TypeRef {
    pub(crate) fn option(inner: TypeRef) -> Self {
        match inner {
            already @ Self::Option(_) => already,
            other => Self::Option(Box::new(other)),
        }
    }

    pub(crate) fn is_option(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    pub(crate) fn non_option(&self) -> &TypeRef {
        match self {
            Self::Option(inner) => inner.non_option(),
            other => other,
        }
    }

    /// True when the rendered type spells out `BTreeMap`, so the generated
    /// file needs a `use std::collections::BTreeMap;` import.
    pub(crate) fn contains_map(&self) -> bool {
        match self {
            Self::Map(_) => true,
            Self::Array(inner) | Self::Option(inner) => inner.contains_map(),
            Self::Constrained { inner, .. } => inner.contains_map(),
            Self::String
            | Self::ParsedString(_)
            | Self::ParsedInteger(_)
            | Self::Integer(_)
            | Self::F32
            | Self::F64
            | Self::Bool
            | Self::JsonValue
            | Self::Range(_)
            | Self::Named(_) => false,
        }
    }
}

impl IntegerType {
    pub(crate) fn min_value(self) -> i128 {
        match self {
            Self::U8 | Self::U16 | Self::U32 | Self::U64 => 0,
            Self::I8 => i128::from(i8::MIN),
            Self::I16 => i128::from(i16::MIN),
            Self::I32 => i128::from(i32::MIN),
            Self::I64 => i128::from(i64::MIN),
        }
    }

    pub(crate) fn max_value(self) -> i128 {
        match self {
            Self::U8 => i128::from(u8::MAX),
            Self::U16 => i128::from(u16::MAX),
            Self::U32 => i128::from(u32::MAX),
            Self::U64 => i128::from(u64::MAX),
            Self::I8 => i128::from(i8::MAX),
            Self::I16 => i128::from(i16::MAX),
            Self::I32 => i128::from(i32::MAX),
            Self::I64 => i128::from(i64::MAX),
        }
    }
}

pub(crate) fn is_array_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Array(_) => true,
        TypeRef::Constrained { inner, .. } => is_array_type(inner.non_option()),
        TypeRef::Option(inner) => is_array_type(inner.non_option()),
        _ => false,
    }
}

impl HttpMethod {
    pub(crate) fn rust_const(self) -> &'static str {
        match self {
            Self::Delete => "DELETE",
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Patch => "PATCH",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Trace => "TRACE",
        }
    }

    pub(crate) fn operation_prefix(self) -> &'static str {
        match self {
            Self::Delete => "delete",
            Self::Get => "get",
            Self::Head => "head",
            Self::Options => "options",
            Self::Patch => "patch",
            Self::Post => "post",
            Self::Put => "put",
            Self::Trace => "trace",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BoolStringMapping, BoolStringMappingError};

    #[test]
    fn bool_string_mapping_preserves_values_and_unknown_as() {
        let mapping = BoolStringMapping::try_new(
            vec!["yes".to_owned(), "true".to_owned(), "yes".to_owned()],
            vec!["no".to_owned(), "false".to_owned(), "no".to_owned()],
            Some(true),
        )
        .expect("mapping should be valid");

        assert_eq!(
            mapping.true_values(),
            ["yes".to_owned(), "true".to_owned(), "yes".to_owned()]
        );
        assert_eq!(
            mapping.false_values(),
            ["no".to_owned(), "false".to_owned(), "no".to_owned()]
        );
        assert_eq!(mapping.unknown_as(), Some(true));
    }

    #[test]
    fn bool_string_mapping_rejects_empty_true_values() {
        assert_eq!(
            BoolStringMapping::try_new(vec![], vec!["no".to_owned()], None),
            Err(BoolStringMappingError::EmptyTrueValues)
        );
    }

    #[test]
    fn bool_string_mapping_rejects_empty_false_values() {
        assert_eq!(
            BoolStringMapping::try_new(vec!["yes".to_owned()], vec![], None),
            Err(BoolStringMappingError::EmptyFalseValues)
        );
    }

    #[test]
    fn bool_string_mapping_reports_the_first_overlap_in_false_value_order() {
        assert_eq!(
            BoolStringMapping::try_new(
                vec!["first-in-true".to_owned(), "first-in-false".to_owned()],
                vec!["first-in-false".to_owned(), "first-in-true".to_owned()],
                Some(false),
            ),
            Err(BoolStringMappingError::OverlappingValue(
                "first-in-false".to_owned()
            ))
        );
    }

    #[test]
    fn bool_string_mapping_preserves_none_unknown_as() {
        let mapping =
            BoolStringMapping::try_new(vec!["yes".to_owned()], vec!["no".to_owned()], None)
                .expect("mapping should be valid");

        assert_eq!(mapping.unknown_as(), None);
    }
}
