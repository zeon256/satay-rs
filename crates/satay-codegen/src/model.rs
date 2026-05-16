#[derive(Debug)]
pub(crate) struct Api {
    pub(crate) server_url: String,
    pub(crate) api_key_security_schemes: Vec<ApiKeySecurityScheme>,
    pub(crate) components: Vec<Component>,
    pub(crate) constrained_types: Vec<ConstrainedType>,
    pub(crate) operations: Vec<Operation>,
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

#[derive(Debug)]
pub(crate) struct Component {
    pub(crate) rust_name: String,
    pub(crate) description: Option<String>,
    pub(crate) kind: ComponentKind,
}

#[derive(Debug)]
pub(crate) enum ComponentKind {
    Struct(Vec<Field>),
    Enum(Vec<EnumVariant>),
    Alias(TypeRef),
    Nutype(ConstrainedType),
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloatLimit {
    pub(crate) value: f64,
    pub(crate) exclusive: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Field {
    pub(crate) wire_name: String,
    pub(crate) rust_name: String,
    pub(crate) description: Option<String>,
    pub(crate) ty: TypeRef,
    pub(crate) required: bool,
    pub(crate) treat_error_as_none: bool,
}

#[derive(Debug)]
pub(crate) struct EnumVariant {
    pub(crate) wire_name: String,
    pub(crate) rust_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TypeRef {
    String,
    ParsedString(ParseAs),
    ParsedInteger(ParseAs),
    I32,
    I64,
    F32,
    F64,
    Bool,
    Array(Box<TypeRef>),
    Named(String),
    Constrained {
        rust_name: String,
        inner: Box<TypeRef>,
    },
    Nullable(Box<TypeRef>),
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
    OffsetDateTime,
}

#[derive(Debug)]
pub(crate) struct Operation {
    pub(crate) fn_name: String,
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

#[derive(Debug)]
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

#[derive(Debug)]
pub(crate) struct ResponseCase {
    pub(crate) status: u16,
    pub(crate) variant_name: String,
    pub(crate) description: Option<String>,
    pub(crate) body: Option<TypeRef>,
}

impl TypeRef {
    pub(crate) fn is_nullable(&self) -> bool {
        matches!(self, Self::Nullable(_))
    }

    pub(crate) fn non_nullable(&self) -> &TypeRef {
        match self {
            Self::Nullable(inner) => inner.non_nullable(),
            other => other,
        }
    }
}

impl ParseAs {
    pub(crate) fn from_wire(value: &str) -> Option<Self> {
        match value {
            "u8" => Some(Self::U8),
            "u16" => Some(Self::U16),
            "u32" => Some(Self::U32),
            "u64" => Some(Self::U64),
            "i8" => Some(Self::I8),
            "i16" => Some(Self::I16),
            "i32" => Some(Self::I32),
            "i64" => Some(Self::I64),
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            "bool" => Some(Self::Bool),
            "offset-datetime" => Some(Self::OffsetDateTime),
            _ => None,
        }
    }
}

pub(crate) fn is_array_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Array(_) => true,
        TypeRef::Constrained { inner, .. } => is_array_type(inner.non_nullable()),
        TypeRef::Nullable(inner) => is_array_type(inner.non_nullable()),
        _ => false,
    }
}

impl HttpMethod {
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        match key {
            "delete" => Some(Self::Delete),
            "get" => Some(Self::Get),
            "head" => Some(Self::Head),
            "options" => Some(Self::Options),
            "patch" => Some(Self::Patch),
            "post" => Some(Self::Post),
            "put" => Some(Self::Put),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }

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
