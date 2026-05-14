#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("unsupported OpenAPI version `{version}`; Satay MVP supports OpenAPI 3.0")]
    UnsupportedOpenApiVersion { version: String },

    #[error("failed to parse OpenAPI YAML/JSON: {0}")]
    ParseDocument(#[source] serde_yaml::Error),

    #[error("failed to normalize OpenAPI document: {0}")]
    NormalizeDocument(#[source] serde_json::Error),

    #[error("unsupported type `{kind}` in schema `{schema}`")]
    UnsupportedComponentType { schema: String, kind: String },

    #[error("schema `{schema}` must declare `type`, `$ref`, `enum`, or `properties`")]
    MissingComponentSchemaType { schema: String },

    #[error("{context} uses enum type `{kind}`; only string enums are supported")]
    UnsupportedEnumType { context: String, kind: String },

    #[error("{context} has a non-array enum")]
    NonArrayEnum { context: String },

    #[error("{context} has an empty enum")]
    EmptyEnum { context: String },

    #[error("{context} contains a non-string enum value; only string enums are supported")]
    NonStringEnumValue { context: String },

    #[error("object schema `{schema}` must declare `properties`")]
    MissingObjectProperties { schema: String },

    #[error("{context} has a non-array `required` field")]
    NonArrayRequired { context: String },

    #[error("{context} has a non-string required field name")]
    NonStringRequiredField { context: String },

    #[error("{context} uses unsupported integer format `{format}`")]
    UnsupportedIntegerFormat { context: String, format: String },

    #[error("{context} uses unsupported number format `{format}`")]
    UnsupportedNumberFormat { context: String, format: String },

    #[error("{context} array schema must declare `items`")]
    MissingArrayItems { context: String },

    #[error("{context} is an inline object schema; move it to components/schemas and use `$ref`")]
    InlineObjectSchema { context: String },

    #[error("{context} is an object without properties; map/object schemas are not supported yet")]
    UnsupportedMapObjectSchema { context: String },

    #[error("{context} uses unsupported schema type `{kind}`")]
    UnsupportedSchemaType { context: String, kind: String },

    #[error("{context} must declare `type`, `$ref`, or `enum`")]
    MissingSchemaType { context: String },

    #[error(
        "{context} uses `pattern`; OpenAPI patterns use ECMA regex syntax and are not safely supported yet"
    )]
    UnsupportedPattern { context: String },

    #[error("{context} has minLength {min_length} greater than maxLength {max_length}")]
    InvalidStringLengthBounds {
        context: String,
        min_length: u64,
        max_length: u64,
    },

    #[error(
        "{context} uses `uniqueItems`; generated Vec-backed types cannot enforce uniqueness yet"
    )]
    UniqueItemsUnsupported { context: String },

    #[error("{context} has minItems {min_items} greater than maxItems {max_items}")]
    InvalidArrayLengthBounds {
        context: String,
        min_items: u64,
        max_items: u64,
    },

    #[error("{context} uses `{keyword}`, which is not safely supported yet")]
    UnsupportedKeyword {
        context: String,
        keyword: &'static str,
    },

    #[error("{context}.{keyword} must be a non-negative integer")]
    InvalidNonNegativeIntegerKeyword {
        context: String,
        keyword: &'static str,
    },

    #[error("{context}.{keyword} must be a boolean")]
    InvalidBooleanKeyword {
        context: String,
        keyword: &'static str,
    },

    #[error("{context}.{exclusive_keyword} requires `{keyword}`")]
    ExclusiveLimitRequiresBound {
        context: String,
        exclusive_keyword: &'static str,
        keyword: &'static str,
    },

    #[error("{context}.{keyword} must be a finite number")]
    InvalidFiniteNumberKeyword {
        context: String,
        keyword: &'static str,
    },

    #[error("{context} must be an integer")]
    ExpectedInteger { context: String },

    #[error("{context} integer bounds do not allow any value")]
    EmptyIntegerBounds { context: String },

    #[error("exclusive integer minimum overflows")]
    ExclusiveIntegerMinimumOverflow,

    #[error("exclusive integer maximum overflows")]
    ExclusiveIntegerMaximumOverflow,

    #[error("{context} number bounds do not allow any value")]
    EmptyNumberBounds { context: String },

    #[error("OpenAPI document must declare `paths`")]
    MissingPaths,

    #[error("operation `{operation_id}` must declare responses")]
    MissingOperationResponses { operation_id: String },

    #[error("{context} must be an array")]
    ExpectedArray { context: String },

    #[error(
        "{context} parameter `{wire_name}` is in `{location}`; only path and query parameters are supported"
    )]
    UnsupportedParameterLocation {
        context: String,
        wire_name: String,
        location: String,
    },

    #[error("{context} parameter `{wire_name}` uses `content`; schema parameters are required")]
    ContentParameterUnsupported { context: String, wire_name: String },

    #[error("{context} parameter `{wire_name}` must declare schema")]
    MissingParameterSchema { context: String, wire_name: String },

    #[error("parameter `{wire_name}` is nullable; nullable parameters are not supported")]
    NullableParameterUnsupported { wire_name: String },

    #[error(
        "path parameter `{wire_name}` is an array; array path parameter styles are not supported"
    )]
    ArrayPathParameterUnsupported { wire_name: String },

    #[error("path parameter `{wire_name}` must set required: true")]
    PathParameterNotRequired { wire_name: String },

    #[error("{context} must declare content")]
    MissingContent { context: String },

    #[error("{context} must declare application/json content")]
    MissingJsonContent { context: String },

    #[error("{context} application/json content must declare schema")]
    MissingJsonSchema { context: String },

    #[error(
        "{context} contains a default response body; default response decoding is not supported yet"
    )]
    DefaultResponseBodyUnsupported { context: String },

    #[error("{context} contains invalid status code `{status}`")]
    InvalidStatusCode { context: String, status: String },

    #[error("{context} contains out-of-range status code `{status_code}`")]
    OutOfRangeStatusCode { context: String, status_code: u16 },

    #[error("{context} {status} response must declare application/json content")]
    MissingResponseJsonContent { context: String, status: String },

    #[error("path `{path}` contains an unclosed parameter")]
    UnclosedPathParameter { path: String },

    #[error("path `{path}` contains an empty parameter")]
    EmptyPathParameter { path: String },

    #[error("path `{path}` uses parameter `{name}` but it is not declared")]
    UndeclaredPathParameter { path: String, name: String },

    #[error("path parameter `{name}` is declared but not used in path `{path}`")]
    UnusedPathParameter { path: String, name: String },

    #[error("generated invalid Rust source: {source}\n--- generated source ---\n{raw}")]
    GeneratedInvalidRust {
        #[source]
        source: syn::Error,
        raw: String,
    },

    #[error("failed to resolve reference `{reference}` in {context}: {source}")]
    ResolveReference {
        reference: String,
        context: String,
        #[source]
        source: Box<Error>,
    },

    #[error("only local references are supported")]
    NonLocalReference,

    #[error("local reference must be a JSON pointer")]
    InvalidLocalReference,

    #[error("missing `{token}`")]
    MissingJsonPointerToken { token: String },

    #[error("reference `{reference}` must point to #/components/{section}/...")]
    InvalidComponentReference {
        reference: String,
        section: &'static str,
    },

    #[error("{context} must be an object")]
    ExpectedObject { context: String },

    #[error("{context}.{field} must be an object")]
    ExpectedObjectField {
        context: String,
        field: &'static str,
    },

    #[error("{context} must declare string field `{field}`")]
    MissingStringField {
        context: String,
        field: &'static str,
    },

    #[error("{context} uses `{keyword}`, which is not in the MVP scope")]
    UnsupportedComposition {
        context: String,
        keyword: &'static str,
    },
}
