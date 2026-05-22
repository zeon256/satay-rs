pub(crate) mod constraint;
mod operation;
mod satay;
mod schema;

use super::resolve::ResolvedDocument;
use crate::error::ValidationError;
use crate::model::{
    EnumVariant, HttpMethod, IntegerType, ParameterLocation, ParseAs, PathSegment, RangeScalar,
    Validation,
};

#[derive(Debug)]
pub(crate) struct ValidatedDocument<'a> {
    pub(crate) resolved: ResolvedDocument<'a>,
    pub(crate) components: Vec<ValidatedComponent>,
    pub(crate) operations: Vec<ValidatedOperation>,
}

#[derive(Debug)]
pub(crate) struct ValidatedComponent {
    pub(crate) schema_name: String,
    pub(crate) description: Option<String>,
    pub(crate) kind: ValidatedComponentKind,
}

#[derive(Debug)]
pub(crate) enum ValidatedComponentKind {
    Reference(String),
    Struct(Vec<ValidatedField>),
    Type(ValidatedType),
}

#[derive(Debug)]
pub(crate) struct ValidatedField {
    pub(crate) wire_name: String,
    pub(crate) description: Option<String>,
    pub(crate) ty: ValidatedType,
    pub(crate) required: bool,
    pub(crate) treat_error_as_none: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedType {
    pub(crate) kind: ValidatedTypeKind,
    pub(crate) nullable: bool,
    pub(crate) validation: Option<Validation>,
    pub(crate) description: Option<String>,
    pub(crate) treat_error_as_none: bool,
}

impl ValidatedType {
    pub(crate) fn named(rust_name: String) -> Self {
        Self {
            kind: ValidatedTypeKind::Named(rust_name),
            nullable: false,
            validation: None,
            description: None,
            treat_error_as_none: false,
        }
    }

    pub(crate) fn is_nullable(&self) -> bool {
        self.nullable
    }

    pub(crate) fn is_array(&self) -> bool {
        matches!(self.kind, ValidatedTypeKind::Array(_))
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ValidatedTypeKind {
    Named(String),
    String,
    ParsedString(ParseAs),
    ParsedInteger(ParseAs),
    Integer(IntegerType),
    F32,
    F64,
    Bool,
    Array(Box<ValidatedType>),
    Enum(Vec<EnumVariant>),
    Range(RangeScalar),
}

#[derive(Debug)]
pub(crate) struct ValidatedOperation {
    pub(crate) operation_id: String,
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
    pub(crate) status: u16,
    pub(crate) description: Option<String>,
    pub(crate) body: Option<ValidatedType>,
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

    let components = schema::validate_components(&document)?;
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
