/// Errors that can occur while validating an OpenAPI document.
///
/// This enum is [`non_exhaustive`](https://doc.rust-lang.org/reference/attributes/type_system.html)
/// so new variants may be added in future releases without a semver break.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    // -- Document and component shape validation --
    /// The OpenAPI version is not supported.
    ///
    /// Error message: `unsupported OpenAPI version `{version}`; Satay MVP supports OpenAPI 3.0`
    #[error("unsupported OpenAPI version `{version}`; Satay MVP supports OpenAPI 3.0")]
    UnsupportedOpenApiVersion { version: String },

    /// A schema component uses a type that is not supported.
    ///
    /// Error message: `unsupported type `{kind}` in schema `{schema}``
    #[error("unsupported type `{kind}` in schema `{schema}`")]
    UnsupportedComponentType { schema: String, kind: String },

    /// A schema component is missing a required `type`, `$ref`, `enum`, or `properties` declaration.
    ///
    /// Error message: `schema `{schema}` must declare `type`, `$ref`, `enum`, or `properties``
    #[error("schema `{schema}` must declare `type`, `$ref`, `enum`, or `properties`")]
    MissingComponentSchemaType { schema: String },

    /// An object schema is missing the required `properties` field.
    ///
    /// Error message: `object schema `{schema}` must declare `properties``
    #[error("object schema `{schema}` must declare `properties`")]
    MissingObjectProperties { schema: String },

    /// The OpenAPI document is missing the required `paths` field.
    ///
    /// Error message: `OpenAPI document must declare `paths``
    #[error("OpenAPI document must declare `paths`")]
    MissingPaths,

    // -- Enum and schema type validation --
    /// A schema uses an enum with a non-string type.
    ///
    /// Error message: `{context} uses enum type `{kind}`; only string enums are supported`
    #[error("{context} uses enum type `{kind}`; only string enums are supported")]
    UnsupportedEnumType { context: String, kind: String },

    /// A schema declares an enum that is not an array.
    ///
    /// Error message: `{context} has a non-array enum`
    #[error("{context} has a non-array enum")]
    NonArrayEnum { context: String },

    /// A schema declares an enum with no values.
    ///
    /// Error message: `{context} has an empty enum`
    #[error("{context} has an empty enum")]
    EmptyEnum { context: String },

    /// A schema enum contains a non-string value.
    ///
    /// Error message: `{context} contains a non-string enum value; only string enums are supported`
    #[error("{context} contains a non-string enum value; only string enums are supported")]
    NonStringEnumValue { context: String },

    /// An `x-satay.enum-variants` value is not an object.
    ///
    /// Error message: `{context}.x-satay.enum-variants must be an object`
    #[error("{context}.x-satay.enum-variants must be an object")]
    InvalidSatayEnumVariants { context: String },

    /// An `x-satay.enum-variants` entry points at a value that is not in the enum.
    ///
    /// Error message: `{context}.x-satay.enum-variants contains `{wire_name}`, which is not declared in the enum`
    #[error(
        "{context}.x-satay.enum-variants contains `{wire_name}`, which is not declared in the enum"
    )]
    UnknownSatayEnumVariantValue { context: String, wire_name: String },

    /// An `x-satay.enum-variants` entry has a non-string Rust variant name.
    ///
    /// Error message: `{context}.x-satay.enum-variants[{wire_name:?}] must be a string`
    #[error("{context}.x-satay.enum-variants[{wire_name:?}] must be a string")]
    InvalidSatayEnumVariantName { context: String, wire_name: String },

    /// Two `x-satay.enum-variants` entries produce the same Rust variant name.
    ///
    /// Error message: `{context}.x-satay.enum-variants maps multiple values to `{rust_name}``
    #[error("{context}.x-satay.enum-variants maps multiple values to `{rust_name}`")]
    DuplicateSatayEnumVariantName { context: String, rust_name: String },

    /// A schema has a `required` field that is not an array.
    ///
    /// Error message: `{context} has a non-array `required` field`
    #[error("{context} has a non-array `required` field")]
    NonArrayRequired { context: String },

    /// A schema `required` array contains a non-string element.
    ///
    /// Error message: `{context} has a non-string required field name`
    #[error("{context} has a non-string required field name")]
    NonStringRequiredField { context: String },

    /// An integer schema uses an unsupported format.
    ///
    /// Error message: `{context} uses unsupported integer format `{format}``
    #[error("{context} uses unsupported integer format `{format}`")]
    UnsupportedIntegerFormat { context: String, format: String },

    /// A number schema uses an unsupported format.
    ///
    /// Error message: `{context} uses unsupported number format `{format}``
    #[error("{context} uses unsupported number format `{format}`")]
    UnsupportedNumberFormat { context: String, format: String },

    /// An `x-satay.parse-as` value is not a supported target type.
    ///
    /// Error message: `{context} uses unsupported x-satay.parse-as `{parse_as}``
    #[error("{context} uses unsupported x-satay.parse-as `{parse_as}`")]
    UnsupportedSatayParseAs { context: String, parse_as: String },

    /// An `x-satay.parse-as` value is not a string.
    ///
    /// Error message: `{context}.x-satay.parse-as must be a string`
    #[error("{context}.x-satay.parse-as must be a string")]
    InvalidSatayParseAs { context: String },

    /// `x-satay.parse-as` was applied to an unsupported wire schema.
    ///
    /// Error message: `{context} uses x-satay.parse-as `{parse_as}` on `{kind}`; supported parse-as wire schemas are string schemas, plus integer schemas for bool`
    #[error(
        "{context} uses x-satay.parse-as `{parse_as}` on `{kind}`; supported parse-as wire schemas are string schemas, plus integer schemas for bool"
    )]
    SatayParseAsRequiresString {
        context: String,
        parse_as: String,
        kind: String,
    },

    /// An array schema is missing the required `items` field.
    ///
    /// Error message: `{context} array schema must declare `items``
    #[error("{context} array schema must declare `items`")]
    MissingArrayItems { context: String },

    /// A schema defines an inline object instead of using a `$ref`.
    ///
    /// Error message: `{context} is an inline object schema; move it to components/schemas and use `$ref``
    #[error("{context} is an inline object schema; move it to components/schemas and use `$ref`")]
    InlineObjectSchema { context: String },

    /// An object schema has no properties (i.e. acts as a map/dictionary), which is unsupported.
    ///
    /// Error message: `{context} is an object without properties; map/object schemas are not supported yet`
    #[error("{context} is an object without properties; map/object schemas are not supported yet")]
    UnsupportedMapObjectSchema { context: String },

    /// A schema uses an unsupported type.
    ///
    /// Error message: `{context} uses unsupported schema type `{kind}``
    #[error("{context} uses unsupported schema type `{kind}`")]
    UnsupportedSchemaType { context: String, kind: String },

    /// A schema is missing a required `type`, `$ref`, or `enum` declaration.
    ///
    /// Error message: `{context} must declare `type`, `$ref`, or `enum``
    #[error("{context} must declare `type`, `$ref`, or `enum`")]
    MissingSchemaType { context: String },

    /// A schema uses a composition keyword (`allOf`, `anyOf`, `oneOf`) that is outside MVP scope.
    ///
    /// Error message: `{context} uses `{keyword}`, which is not in the MVP scope`
    #[error("{context} uses `{keyword}`, which is not in the MVP scope")]
    UnsupportedComposition {
        context: String,
        keyword: &'static str,
    },

    // -- Schema constraint validation --
    /// A string schema specifies a `minLength` greater than its `maxLength`.
    ///
    /// Error message: `{context} has minLength {min_length} greater than maxLength {max_length}`
    #[error("{context} has minLength {min_length} greater than maxLength {max_length}")]
    InvalidStringLengthBounds {
        context: String,
        min_length: u64,
        max_length: u64,
    },

    /// A schema uses `uniqueItems`, which cannot be enforced by generated `Vec`-backed types.
    ///
    /// Error message: `{context} uses `uniqueItems`; generated Vec-backed types cannot enforce uniqueness yet`
    #[error(
        "{context} uses `uniqueItems`; generated Vec-backed types cannot enforce uniqueness yet"
    )]
    UniqueItemsUnsupported { context: String },

    /// An array schema specifies `minItems` greater than `maxItems`.
    ///
    /// Error message: `{context} has minItems {min_items} greater than maxItems {max_items}`
    #[error("{context} has minItems {min_items} greater than maxItems {max_items}")]
    InvalidArrayLengthBounds {
        context: String,
        min_items: u64,
        max_items: u64,
    },

    /// A schema uses a keyword that is not safely supported.
    ///
    /// Error message: `{context} uses `{keyword}`, which is not safely supported yet`
    #[error("{context} uses `{keyword}`, which is not safely supported yet")]
    UnsupportedKeyword {
        context: String,
        keyword: &'static str,
    },

    /// A schema keyword that must be a non-negative integer has an invalid value.
    ///
    /// Error message: `{context}.{keyword} must be a non-negative integer`
    #[error("{context}.{keyword} must be a non-negative integer")]
    InvalidNonNegativeIntegerKeyword {
        context: String,
        keyword: &'static str,
    },

    /// A schema keyword that must be a boolean has an invalid value.
    ///
    /// Error message: `{context}.{keyword} must be a boolean`
    #[error("{context}.{keyword} must be a boolean")]
    InvalidBooleanKeyword {
        context: String,
        keyword: &'static str,
    },

    /// An `exclusiveMinimum`/`exclusiveMaximum` keyword is present but the corresponding bound is missing.
    ///
    /// Error message: `{context}.{exclusive_keyword} requires `{keyword}``
    #[error("{context}.{exclusive_keyword} requires `{keyword}`")]
    ExclusiveLimitRequiresBound {
        context: String,
        exclusive_keyword: &'static str,
        keyword: &'static str,
    },

    /// A schema keyword that must be a finite number has a non-finite value.
    ///
    /// Error message: `{context}.{keyword} must be a finite number`
    #[error("{context}.{keyword} must be a finite number")]
    InvalidFiniteNumberKeyword {
        context: String,
        keyword: &'static str,
    },

    /// A value expected to be an integer is not.
    ///
    /// Error message: `{context} must be an integer`
    #[error("{context} must be an integer")]
    ExpectedInteger { context: String },

    /// Integer bounds (minimum/maximum) do not permit any value.
    ///
    /// Error message: `{context} integer bounds do not allow any value`
    #[error("{context} integer bounds do not allow any value")]
    EmptyIntegerBounds { context: String },

    /// An exclusive integer minimum overflows `i64`.
    ///
    /// Error message: `exclusive integer minimum overflows`
    #[error("exclusive integer minimum overflows")]
    ExclusiveIntegerMinimumOverflow,

    /// An exclusive integer maximum overflows `i64`.
    ///
    /// Error message: `exclusive integer maximum overflows`
    #[error("exclusive integer maximum overflows")]
    ExclusiveIntegerMaximumOverflow,

    /// Number bounds (minimum/maximum) do not permit any value.
    ///
    /// Error message: `{context} number bounds do not allow any value`
    #[error("{context} number bounds do not allow any value")]
    EmptyNumberBounds { context: String },

    // -- Operation, parameter, and response validation --
    /// An operation does not declare any responses.
    ///
    /// Error message: `operation `{operation_id}` must declare responses`
    #[error("operation `{operation_id}` must declare responses")]
    MissingOperationResponses { operation_id: String },

    /// A value expected to be an array is not.
    ///
    /// Error message: `{context} must be an array`
    #[error("{context} must be an array")]
    ExpectedArray { context: String },

    /// A parameter uses an unsupported location (e.g. cookie) instead of path, query, or header.
    ///
    /// Error message: `{context} parameter `{wire_name}` is in `{location}`; only path, query, and header parameters are supported`
    #[error(
        "{context} parameter `{wire_name}` is in `{location}`; only path, query, and header parameters are supported"
    )]
    UnsupportedParameterLocation {
        context: String,
        wire_name: String,
        location: String,
    },

    /// A parameter uses `content` instead of `schema`.
    ///
    /// Error message: `{context} parameter `{wire_name}` uses `content`; schema parameters are required`
    #[error("{context} parameter `{wire_name}` uses `content`; schema parameters are required")]
    ContentParameterUnsupported { context: String, wire_name: String },

    /// A parameter is missing a required `schema` declaration.
    ///
    /// Error message: `{context} parameter `{wire_name}` must declare schema`
    #[error("{context} parameter `{wire_name}` must declare schema")]
    MissingParameterSchema { context: String, wire_name: String },

    /// A parameter is nullable, which is not supported.
    ///
    /// Error message: `parameter `{wire_name}` is nullable; nullable parameters are not supported`
    #[error("parameter `{wire_name}` is nullable; nullable parameters are not supported")]
    NullableParameterUnsupported { wire_name: String },

    /// A path parameter is an array, which is not supported.
    ///
    /// Error message: `path parameter `{wire_name}` is an array; array path parameter styles are not supported`
    #[error(
        "path parameter `{wire_name}` is an array; array path parameter styles are not supported"
    )]
    ArrayPathParameterUnsupported { wire_name: String },

    /// A header parameter is an array, which is not supported.
    ///
    /// Error message: `header parameter `{wire_name}` is an array; array header parameter styles are not supported`
    #[error(
        "header parameter `{wire_name}` is an array; array header parameter styles are not supported"
    )]
    ArrayHeaderParameterUnsupported { wire_name: String },

    /// A path parameter does not set `required: true`.
    ///
    /// Error message: `path parameter `{wire_name}` must set required: true`
    #[error("path parameter `{wire_name}` must set required: true")]
    PathParameterNotRequired { wire_name: String },

    /// A context is missing a required `content` declaration.
    ///
    /// Error message: `{context} must declare content`
    #[error("{context} must declare content")]
    MissingContent { context: String },

    /// A context is missing the required `application/json` content type.
    ///
    /// Error message: `{context} must declare application/json content`
    #[error("{context} must declare application/json content")]
    MissingJsonContent { context: String },

    /// A context's `application/json` content is missing a schema.
    ///
    /// Error message: `{context} application/json content must declare schema`
    #[error("{context} application/json content must declare schema")]
    MissingJsonSchema { context: String },

    /// A response body uses the `default` status, which is not yet supported for decoding.
    ///
    /// Error message: `{context} contains a default response body; default response decoding is not supported yet`
    #[error(
        "{context} contains a default response body; default response decoding is not supported yet"
    )]
    DefaultResponseBodyUnsupported { context: String },

    /// A response contains an invalid HTTP status code string.
    ///
    /// Error message: `{context} contains invalid status code `{status}``
    #[error("{context} contains invalid status code `{status}`")]
    InvalidStatusCode { context: String, status: String },

    /// A response contains a status code outside the valid 100–599 range.
    ///
    /// Error message: `{context} contains out-of-range status code `{status_code}``
    #[error("{context} contains out-of-range status code `{status_code}`")]
    OutOfRangeStatusCode { context: String, status_code: u16 },

    /// A response for a given status code is missing `application/json` content.
    ///
    /// Error message: `{context} {status} response must declare application/json content`
    #[error("{context} {status} response must declare application/json content")]
    MissingResponseJsonContent { context: String, status: String },

    /// A path template contains a parameter that is never closed.
    ///
    /// Error message: `path `{path}` contains an unclosed parameter`
    #[error("path `{path}` contains an unclosed parameter")]
    UnclosedPathParameter { path: String },

    /// A path template contains an empty parameter (e.g. `{}`).
    ///
    /// Error message: `path `{path}` contains an empty parameter`
    #[error("path `{path}` contains an empty parameter")]
    EmptyPathParameter { path: String },

    /// A path template references a parameter that is not declared in the operation's parameters.
    ///
    /// Error message: `path `{path}` uses parameter `{name}` but it is not declared`
    #[error("path `{path}` uses parameter `{name}` but it is not declared")]
    UndeclaredPathParameter { path: String, name: String },

    /// A parameter is declared for a path but never used in the path template.
    ///
    /// Error message: `path parameter `{name}` is declared but not used in path `{path}``
    #[error("path parameter `{name}` is declared but not used in path `{path}`")]
    UnusedPathParameter { path: String, name: String },

    // -- Reference resolution and JSON shape validation --
    /// A `$ref` could not be resolved because the referenced component failed validation.
    ///
    /// Error message: `failed to resolve reference `{reference}` in {context}: {source}`
    #[error("failed to resolve reference `{reference}` in {context}: {source}")]
    ResolveReference {
        reference: String,
        context: String,
        #[source]
        source: Box<ValidationError>,
    },

    /// A reference points to an external document; only local (`#`) references are supported.
    ///
    /// Error message: `only local references are supported`
    #[error("only local references are supported")]
    NonLocalReference,

    /// A local reference is not a valid JSON pointer.
    ///
    /// Error message: `local reference must be a JSON pointer`
    #[error("local reference must be a JSON pointer")]
    InvalidLocalReference,

    /// A JSON pointer is missing a required token segment.
    ///
    /// Error message: `missing `{token}``
    #[error("missing `{token}`")]
    MissingJsonPointerToken { token: String },

    /// A `$ref` does not point to the expected `#/components/{section}/…` path.
    ///
    /// Error message: `reference `{reference}` must point to #/components/{section}/...`
    #[error("reference `{reference}` must point to #/components/{section}/...")]
    InvalidComponentReference {
        reference: String,
        section: &'static str,
    },

    /// A value expected to be an object is not.
    ///
    /// Error message: `{context} must be an object`
    #[error("{context} must be an object")]
    ExpectedObject { context: String },

    /// A nested field expected to be an object is not.
    ///
    /// Error message: `{context}.{field} must be an object`
    #[error("{context}.{field} must be an object")]
    ExpectedObjectField {
        context: String,
        field: &'static str,
    },

    /// A required string field is missing from an object.
    ///
    /// Error message: `{context} must declare string field `{field}``
    #[error("{context} must declare string field `{field}`")]
    MissingStringField {
        context: String,
        field: &'static str,
    },
}
