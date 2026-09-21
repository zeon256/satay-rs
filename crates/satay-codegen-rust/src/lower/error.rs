//! Parser-independent validation errors produced by Rust lowering.
//!
//! This enum mirrors the subset of the facade `ValidationError` surface that
//! Rust lowering can reject a semantic graph with. Every variant carries
//! parser-independent typed payloads; the facade reconstructs its public
//! diagnostics from these shapes without stringifying anything.

/// Rust-support validation failures raised while lowering the semantic IR.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    /// A schema enum contains a non-string value.
    #[error("{context} contains a non-string enum value; only string enums are supported")]
    NonStringEnumValue { context: String },

    /// A coordinate selector is missing, inconsistent, or selects an invalid target shape.
    #[error("{context} has invalid x-satay coordinates configuration: {reason}")]
    InvalidSatayCoordinates { context: String, reason: String },

    /// A field-local coordinate codec would be lost in this value context.
    #[error(
        "{context} uses a coordinates string codec outside a direct object property; coordinate codecs require a serde-bearing struct field"
    )]
    SatayCoordinatesRequireStructField { context: String },

    /// An `x-satay.enum-variants` entry uses a name reserved for generated fallback variants.
    #[error(
        "{context}.x-satay.enum-variants[{wire_name:?}] uses reserved fallback variant `{rust_name}`"
    )]
    ReservedSatayEnumVariantName {
        context: String,
        wire_name: String,
        rust_name: String,
    },

    /// Two `x-satay.enum-variants` entries produce the same Rust variant name.
    #[error("{context}.x-satay.enum-variants maps multiple values to `{rust_name}`")]
    DuplicateSatayEnumVariantName { context: String, rust_name: String },

    /// An integer schema uses an unsupported format.
    #[error("{context} uses unsupported integer format `{format}`")]
    UnsupportedIntegerFormat { context: String, format: String },

    /// A number schema uses an unsupported format.
    #[error("{context} uses unsupported number format `{format}`")]
    UnsupportedNumberFormat { context: String, format: String },

    /// `x-satay.none-if` was not paired with a string-backed parser.
    #[error("{context} uses x-satay.none-if without a string-backed x-satay.parse-as")]
    SatayNoneIfRequiresParsedString { context: String },

    /// `x-satay.treat-error-as-none` was applied outside an object property.
    #[error("{context} uses x-satay.treat-error-as-none outside an object property")]
    SatayTreatErrorAsNoneRequiresObjectProperty { context: String },

    /// An explicit property identifier collides with another Rust field after normalization.
    #[error(
        "{context} maps properties `{first_property}` and `{second_property}` to duplicate Rust field `{rust_name}`"
    )]
    DuplicateSatayIdentifierRustField {
        context: String,
        first_property: String,
        second_property: String,
        rust_name: String,
    },

    /// A schema defines an inline object instead of using a `$ref`.
    #[error("{context} is an inline object schema; move it to components/schemas and use `$ref`")]
    InlineObjectSchema { context: String },

    /// An object schema has no properties (i.e. acts as a map/dictionary), which is unsupported.
    #[error(
        "{context} is an object with neither `properties` nor a supported `additionalProperties` schema"
    )]
    UnsupportedMapObjectSchema { context: String },

    /// A schema uses an unsupported type.
    #[error("{context} uses unsupported schema type `{kind}`")]
    UnsupportedSchemaType { context: String, kind: String },

    /// A schema uses a composition keyword (`allOf`, `anyOf`, `oneOf`) in an unsupported context.
    #[error("{context} uses `{keyword}`, which is not supported in this context")]
    UnsupportedComposition {
        context: String,
        keyword: &'static str,
    },

    /// An `allOf` branch cannot be flattened into a generated Rust struct.
    #[error(
        "{context}.allOf[{index}] must be a local component schema reference or object schema with properties"
    )]
    UnsupportedAllOfBranch { context: String, index: usize },

    /// Two `allOf` branches declare the same object property.
    #[error("{context} declares duplicate `allOf` property `{property}`")]
    DuplicateAllOfProperty { context: String, property: String },

    /// `allOf` component schemas form a recursive flattening cycle.
    #[error("{context} forms a recursive `allOf` cycle through schema `{schema}`")]
    RecursiveAllOf { context: String, schema: String },

    /// A discriminator union branch component recursively contains its own union.
    #[error("{context} forms a recursive discriminator cycle through branch schema `{schema}`")]
    RecursiveDiscriminatorBranch { context: String, schema: String },

    /// An `anyOf` branch is not a supported union branch.
    #[error(
        "{context}.anyOf[{index}] must be a local component schema reference, inline string enum, inline primitive schema, or null schema"
    )]
    UnsupportedAnyOfBranch { context: String, index: usize },

    /// A `oneOf` branch is not a supported union branch.
    #[error(
        "{context}.oneOf[{index}] must be a local component schema reference, inline string enum, inline primitive schema, or null schema"
    )]
    UnsupportedOneOfBranch { context: String, index: usize },

    /// A plain `anyOf` or `oneOf` union has more than one null branch.
    #[error("{context}.{keyword}[{index}] duplicates the union null branch")]
    DuplicateUnionNullBranch {
        context: String,
        keyword: &'static str,
        index: usize,
    },

    /// An open string enum `anyOf` repeats an enum or `const` value across branches.
    #[error(
        "{context} declares duplicate open string enum value `{value}` across `anyOf` branches"
    )]
    DuplicateOpenStringEnumValue { context: String, value: String },

    /// A nullable plain `anyOf` or `oneOf` union has no non-null branches.
    #[error("{context}.{keyword} must declare at least one non-null branch")]
    NullableUnionWithoutVariants {
        context: String,
        keyword: &'static str,
    },

    /// A plain `anyOf` or `oneOf` union has a branch that is statically shadowed by an earlier branch.
    #[error(
        "{context}.{keyword}[{index}] is shadowed by earlier branch {shadowed_by} under ordered serde untagged deserialization"
    )]
    ShadowedUnionBranch {
        context: String,
        keyword: &'static str,
        index: usize,
        shadowed_by: usize,
    },

    /// `anyOf` component schemas form a recursive union cycle.
    #[error("{context} forms a recursive `anyOf` cycle through schema `{schema}`")]
    RecursiveAnyOf { context: String, schema: String },

    /// A discriminator union branch is not a local component schema reference.
    #[error(
        "{context}.{keyword}[{index}] must be a local component schema reference when using `discriminator`"
    )]
    UnsupportedDiscriminatorBranch {
        context: String,
        keyword: &'static str,
        index: usize,
    },

    /// A discriminator union branch target does not generate as an object struct.
    #[error("{context} discriminator branch `{schema}` must be an object struct component")]
    DiscriminatorBranchNotObject { context: String, schema: String },

    /// A discriminator branch object contains an invalid embedded discriminator property.
    #[error("{context} discriminator branch `{schema}` property `{property}` must be {expected}")]
    InvalidDiscriminatorProperty {
        context: String,
        schema: String,
        property: String,
        expected: &'static str,
    },

    /// Multiple discriminator mapping values target the same union branch schema.
    #[error("{context}.discriminator.mapping maps multiple values to branch schema `{schema}`")]
    DuplicateDiscriminatorMapping { context: String, schema: String },

    /// A discriminator mapping value disagrees with a branch's embedded discriminator property value.
    #[error(
        "{context}.discriminator.mapping maps value `{value}` to branch schema `{schema}`, but the branch declares discriminator value `{actual}`"
    )]
    DiscriminatorMappingValueMismatch {
        context: String,
        schema: String,
        value: String,
        actual: String,
    },

    /// Multiple discriminator branches resolve to the same discriminator value after implicit defaults are applied.
    #[error("{context}.discriminator resolves multiple branch schemas to value `{value}`")]
    DuplicateDiscriminatorValue { context: String, value: String },

    /// A string schema specifies a `minLength` greater than its `maxLength`.
    #[error("{context} has minLength {min_length} greater than maxLength {max_length}")]
    InvalidStringLengthBounds {
        context: String,
        min_length: u64,
        max_length: u64,
    },

    /// A schema uses `uniqueItems`, which cannot be enforced by generated `Vec`-backed types.
    #[error(
        "{context} uses `uniqueItems`; generated Vec-backed types cannot enforce uniqueness yet"
    )]
    UniqueItemsUnsupported { context: String },

    /// An array schema specifies `minItems` greater than `maxItems`.
    #[error("{context} has minItems {min_items} greater than maxItems {max_items}")]
    InvalidArrayLengthBounds {
        context: String,
        min_items: u64,
        max_items: u64,
    },

    /// A schema uses a keyword that is not safely supported.
    #[error("{context} uses `{keyword}`, which is not safely supported yet")]
    UnsupportedKeyword { context: String, keyword: String },

    /// A schema keyword that must be a finite number has a non-finite value.
    #[error("{context}.{keyword} must be a finite number")]
    InvalidFiniteNumberKeyword {
        context: String,
        keyword: &'static str,
    },

    /// A value expected to be an integer is not.
    #[error("{context} must be an integer")]
    ExpectedInteger { context: String },

    /// Integer bounds (minimum/maximum) do not permit any value.
    #[error("{context} integer bounds do not allow any value")]
    EmptyIntegerBounds { context: String },

    /// An exclusive integer minimum overflows `i64`.
    #[error("exclusive integer minimum overflows")]
    ExclusiveIntegerMinimumOverflow,

    /// An exclusive integer maximum overflows `i64`.
    #[error("exclusive integer maximum overflows")]
    ExclusiveIntegerMaximumOverflow,

    /// Number bounds (minimum/maximum) do not permit any value.
    #[error("{context} number bounds do not allow any value")]
    EmptyNumberBounds { context: String },

    /// A parameter uses an unsupported location (e.g. cookie) instead of path, query, or header.
    #[error(
        "{context} parameter `{wire_name}` is in `{location}`; only path, query, and header parameters are supported"
    )]
    UnsupportedParameterLocation {
        context: String,
        wire_name: String,
        location: String,
    },

    /// A parameter schema declares a default that cannot be represented by the generated input.
    #[error("parameter `{wire_name}` has invalid default {value}: {reason}")]
    InvalidParameterDefault {
        wire_name: String,
        value: String,
        reason: String,
    },

    /// A parameter is nullable, which is not supported.
    #[error("parameter `{wire_name}` is nullable; nullable parameters are not supported")]
    NullableParameterUnsupported { wire_name: String },

    /// A parameter uses `anyOf`, which is not supported for URI/header encoding yet.
    #[error("parameter `{wire_name}` uses `anyOf`; anyOf parameters are not supported yet")]
    AnyOfParameterUnsupported { wire_name: String },

    /// A parameter is a map or arbitrary JSON value, which has no URI/header encoding.
    #[error("parameter `{wire_name}` is a map or JSON value; map parameters are not supported")]
    MapParameterUnsupported { wire_name: String },

    /// A path parameter is an array, which is not supported.
    #[error(
        "path parameter `{wire_name}` is an array; array path parameter styles are not supported"
    )]
    ArrayPathParameterUnsupported { wire_name: String },

    /// A header parameter is an array, which is not supported.
    #[error(
        "header parameter `{wire_name}` is an array; array header parameter styles are not supported"
    )]
    ArrayHeaderParameterUnsupported { wire_name: String },

    /// A context is missing a required `content` declaration.
    #[error("{context} must declare content")]
    MissingContent { context: String },

    /// A context is missing the required `application/json` content type.
    #[error("{context} must declare application/json content")]
    MissingJsonContent { context: String },

    /// A context's `application/json` content is missing a schema.
    #[error("{context} application/json content must declare schema")]
    MissingJsonSchema { context: String },

    /// A response body uses the `default` status, which is not yet supported for decoding.
    #[error(
        "{context} contains a default response body; default response decoding is not supported yet"
    )]
    DefaultResponseBodyUnsupported { context: String },

    /// A response contains an invalid HTTP status code string.
    #[error("{context} contains invalid status code `{status}`")]
    InvalidStatusCode { context: String, status: String },

    /// A response contains a status code outside the valid 100–599 range.
    #[error("{context} contains out-of-range status code `{status_code}`")]
    OutOfRangeStatusCode { context: String, status_code: u16 },

    /// A wildcard response status class is outside the valid 1–5 range.
    #[error("{context} contains out-of-range status class `{class}`; expected 1 through 5")]
    OutOfRangeStatusClass { context: String, class: u8 },

    /// A mapped response projection does not lower to an array.
    #[error("{context} mapped response projection must lower to an array")]
    MappedResponseProjectionRequiresArray { context: String },

    /// A response for a given status code is missing `application/json` content.
    #[error("{context} {status} response must declare application/json content")]
    MissingResponseJsonContent { context: String, status: String },

    /// `x-satay.output` was configured on an operation with no JSON response body.
    #[error("operation `{operation_id}` uses x-satay.output but has no JSON response body")]
    SatayOutputRequiresResponseBody { operation_id: String },

    /// A path template contains a parameter that is never closed.
    #[error("path `{path}` contains an unclosed parameter")]
    UnclosedPathParameter { path: String },

    /// A path template references a parameter that is not declared in the operation's parameters.
    #[error("path `{path}` uses parameter `{name}` but it is not declared")]
    UndeclaredPathParameter { path: String, name: String },

    /// A parameter is declared for a path but never used in the path template.
    #[error("path parameter `{name}` is declared but not used in path `{path}`")]
    UnusedPathParameter { path: String, name: String },
}
