//! Structured failures retained at their semantic encounter position.

/// A frontend failure with its original contextual payload.
/// Rust naming and representation failures remain in the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// The `OpenAPI` version is not supported.
    UnsupportedOpenApiVersion {
        /// Original version supplied by the frontend.
        version: String,
    },
    /// A schema component uses a type that is not supported.
    UnsupportedComponentType {
        /// Original schema supplied by the frontend.
        schema: String,
        /// Original kind supplied by the frontend.
        kind: String,
    },
    /// A schema component is missing a required `type`, `$ref`, `enum`, or `properties` declaration.
    MissingComponentSchemaType {
        /// Original schema supplied by the frontend.
        schema: String,
    },
    /// An object schema is missing the required `properties` field.
    MissingObjectProperties {
        /// Original schema supplied by the frontend.
        schema: String,
    },
    /// The `OpenAPI` document is missing the required `paths` field.
    MissingPaths,
    /// A schema uses an enum with a non-string type.
    UnsupportedEnumType {
        /// Original context supplied by the frontend.
        context: String,
        /// Original kind supplied by the frontend.
        kind: String,
    },
    /// A schema declares an enum that is not an array.
    NonArrayEnum {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A schema declares an enum with no values.
    EmptyEnum {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A schema enum contains a non-string value.
    NonStringEnumValue {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A schema declares a `const` value that is not one of its `enum` values.
    ConstNotInEnum {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A typed specification extension could not be deserialized.
    InvalidExtension {
        /// Original context supplied by the frontend.
        context: String,
        /// Original path supplied by the frontend.
        path: String,
        /// Message from extension deserialization of an owned JSON value.
        /// Such failures have the data category and no text coordinates.
        source: String,
    },
    /// `x-satay.enum-variants` was applied to a schema without enum values.
    SatayEnumVariantsRequireEnum {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// `x-satay.parse-as` was combined with enum values.
    SatayParseAsWithEnum {
        /// Original context supplied by the frontend.
        context: String,
        /// Original parse as supplied by the frontend.
        parse_as: String,
    },
    /// An `x-satay` type option was combined with enum values even though the option has no enum consumer.
    SatayOptionUnsupportedWithEnum {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
    },
    /// A coordinate selector is missing, inconsistent, or selects an invalid target shape.
    InvalidSatayCoordinates {
        /// Original context supplied by the frontend.
        context: String,
        /// Original reason supplied by the frontend.
        reason: String,
    },
    /// A coordinate-specific option was configured without the coordinate codec.
    SatayOptionRequiresCoordinates {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
    },
    /// An `x-satay.enum-variants` entry points at a value that is not in the enum.
    UnknownSatayEnumVariantValue {
        /// Original context supplied by the frontend.
        context: String,
        /// Original wire name supplied by the frontend.
        wire_name: String,
    },
    /// A schema has a `required` field that is not an array.
    NonArrayRequired {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A schema `required` array contains a non-string element.
    NonStringRequiredField {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A number schema uses an unsupported format.
    UnsupportedNumberFormat {
        /// Original context supplied by the frontend.
        context: String,
        /// Original format supplied by the frontend.
        format: String,
    },
    /// `x-satay.parse-as` was applied to an unsupported wire schema.
    SatayParseAsRequiresString {
        /// Original context supplied by the frontend.
        context: String,
        /// Original parse as supplied by the frontend.
        parse_as: String,
        /// Original kind supplied by the frontend.
        kind: String,
    },
    /// Integer-backed boolean parsing was combined with `x-satay.integer-type`.
    SatayParseAsBoolWithIntegerType {
        /// Original context supplied by the frontend.
        context: String,
        /// Original integer type supplied by the frontend.
        integer_type: String,
    },
    /// An `x-satay.none-if` array is empty.
    EmptySatayNoneIf {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// `x-satay.none-if` was applied outside a struct property.
    SatayNoneIfRequiresStructField {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// `x-satay.none-if` was not paired with a string-backed parser.
    SatayNoneIfRequiresParsedString {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// `x-satay.none-if` and `x-satay.treat-error-as-none` were combined.
    ConflictingSatayNoneHandling {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A boolean string mapping was not paired with a string-backed bool parser.
    SatayBoolMappingRequiresParsedStringBool {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A boolean string mapping omitted one of its required value lists.
    IncompleteSatayBoolMapping {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A boolean string mapping contains an empty value list.
    EmptySatayBoolMapping {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
    },
    /// A value appears in both true and false boolean string mappings.
    OverlappingSatayBoolMapping {
        /// Original context supplied by the frontend.
        context: String,
        /// Original value supplied by the frontend.
        value: String,
    },
    /// A boolean mapping value is also configured as a null sentinel.
    OverlappingSatayBoolMappingNoneIf {
        /// Original context supplied by the frontend.
        context: String,
        /// Original value supplied by the frontend.
        value: String,
    },
    /// `x-satay.treat-error-as-none` was applied outside an object property.
    SatayTreatErrorAsNoneRequiresObjectProperty {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// `x-satay.ignore` was applied outside an object property.
    SatayIgnoreRequiresObjectProperty {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// `x-satay.ignore: true` was combined with another schema-level `x-satay` option.
    SatayOptionConflictsWithIgnore {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
    },
    /// `x-satay.identifier` was applied outside an object property.
    SatayIdentifierRequiresObjectProperty {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// `x-satay.integer-type` was applied to a non-integer schema.
    SatayIntegerTypeRequiresInteger {
        /// Original context supplied by the frontend.
        context: String,
        /// Original integer type supplied by the frontend.
        integer_type: String,
        /// Original kind supplied by the frontend.
        kind: String,
    },
    /// An array schema is missing the required `items` field.
    MissingArrayItems {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A schema defines an inline object instead of using a `$ref`.
    InlineObjectSchema {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// An object schema has no properties (i.e. acts as a map/dictionary), which is unsupported.
    UnsupportedMapObjectSchema {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A schema uses an unsupported type.
    UnsupportedSchemaType {
        /// Original context supplied by the frontend.
        context: String,
        /// Original kind supplied by the frontend.
        kind: String,
    },
    /// A schema is missing a required `type`, `$ref`, or `enum` declaration.
    MissingSchemaType {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A JSON Schema boolean schema was used; Satay has no IR equivalent yet.
    UnsupportedBooleanSchema {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A schema type array contains more than one non-null type.
    MultipleNonNullSchemaTypesUnsupported {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A schema uses a composition keyword (`allOf`, `anyOf`, `oneOf`) in an unsupported context.
    UnsupportedComposition {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
    },
    /// A schema uses a sibling beside `$ref` that Satay cannot apply.
    UnsupportedRefSiblingKeyword {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: String,
    },
    /// An `allOf` schema combines supported struct flattening with another schema keyword.
    UnsupportedAllOfSiblingKeyword {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: String,
    },
    /// An `allOf` branch cannot be flattened into a generated Rust struct.
    UnsupportedAllOfBranch {
        /// Original context supplied by the frontend.
        context: String,
        /// Original index supplied by the frontend.
        index: usize,
    },
    /// Two `allOf` branches declare the same object property.
    DuplicateAllOfProperty {
        /// Original context supplied by the frontend.
        context: String,
        /// Original property supplied by the frontend.
        property: String,
    },
    /// `allOf` component schemas form a recursive flattening cycle.
    RecursiveAllOf {
        /// Original context supplied by the frontend.
        context: String,
        /// Original schema supplied by the frontend.
        schema: String,
    },
    /// A discriminator union branch component recursively contains its own union.
    RecursiveDiscriminatorBranch {
        /// Original context supplied by the frontend.
        context: String,
        /// Original schema supplied by the frontend.
        schema: String,
    },
    /// An `anyOf` schema combines a supported union with another schema keyword.
    UnsupportedAnyOfSiblingKeyword {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: String,
    },
    /// An `anyOf` branch is not a supported union branch.
    UnsupportedAnyOfBranch {
        /// Original context supplied by the frontend.
        context: String,
        /// Original index supplied by the frontend.
        index: usize,
    },
    /// A `oneOf` schema combines a supported union with another schema keyword.
    UnsupportedOneOfSiblingKeyword {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: String,
    },
    /// A `oneOf` branch is not a supported union branch.
    UnsupportedOneOfBranch {
        /// Original context supplied by the frontend.
        context: String,
        /// Original index supplied by the frontend.
        index: usize,
    },
    /// A plain `anyOf` or `oneOf` union has more than one null branch.
    DuplicateUnionNullBranch {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
        /// Original index supplied by the frontend.
        index: usize,
    },
    /// An open string enum `anyOf` repeats an enum or `const` value across branches.
    DuplicateOpenStringEnumValue {
        /// Original context supplied by the frontend.
        context: String,
        /// Original value supplied by the frontend.
        value: String,
    },
    /// A nullable plain `anyOf` or `oneOf` union has no non-null branches.
    NullableUnionWithoutVariants {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
    },
    /// A plain `anyOf` or `oneOf` union has a branch that is statically shadowed by an earlier branch.
    ShadowedUnionBranch {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
        /// Original index supplied by the frontend.
        index: usize,
        /// Original shadowed by supplied by the frontend.
        shadowed_by: usize,
    },
    /// A composition schema declares no branches.
    EmptyAnyOf {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// `anyOf` component schemas form a recursive union cycle.
    RecursiveAnyOf {
        /// Original context supplied by the frontend.
        context: String,
        /// Original schema supplied by the frontend.
        schema: String,
    },
    /// A discriminator union does not use exactly one non-empty `anyOf` or `oneOf` branch list.
    InvalidDiscriminatorUnion {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A discriminator union branch is not a local component schema reference.
    UnsupportedDiscriminatorBranch {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
        /// Original index supplied by the frontend.
        index: usize,
    },
    /// A discriminator union branch target does not generate as an object struct.
    DiscriminatorBranchNotObject {
        /// Original context supplied by the frontend.
        context: String,
        /// Original schema supplied by the frontend.
        schema: String,
    },
    /// A discriminator branch object contains the discriminator property.
    DiscriminatorPropertyConflict {
        /// Original context supplied by the frontend.
        context: String,
        /// Original schema supplied by the frontend.
        schema: String,
        /// Original property supplied by the frontend.
        property: String,
    },
    /// A discriminator branch object contains an invalid embedded discriminator property.
    InvalidDiscriminatorProperty {
        /// Original context supplied by the frontend.
        context: String,
        /// Original schema supplied by the frontend.
        schema: String,
        /// Original property supplied by the frontend.
        property: String,
        /// Original expected supplied by the frontend.
        expected: &'static str,
    },
    /// A discriminator mapping entry targets a non-local schema or a schema outside the union branches.
    InvalidDiscriminatorMapping {
        /// Original context supplied by the frontend.
        context: String,
        /// Original value supplied by the frontend.
        value: String,
        /// Original target supplied by the frontend.
        target: String,
    },
    /// Multiple discriminator mapping values target the same union branch schema.
    DuplicateDiscriminatorMapping {
        /// Original context supplied by the frontend.
        context: String,
        /// Original schema supplied by the frontend.
        schema: String,
    },
    /// A discriminator mapping value disagrees with a branch's embedded discriminator property value.
    DiscriminatorMappingValueMismatch {
        /// Original context supplied by the frontend.
        context: String,
        /// Original schema supplied by the frontend.
        schema: String,
        /// Original value supplied by the frontend.
        value: String,
        /// Original actual supplied by the frontend.
        actual: String,
    },
    /// Multiple discriminator branches resolve to the same discriminator value after implicit defaults are applied.
    DuplicateDiscriminatorValue {
        /// Original context supplied by the frontend.
        context: String,
        /// Original value supplied by the frontend.
        value: String,
    },
    /// A string schema specifies a `minLength` greater than its `maxLength`.
    InvalidStringLengthBounds {
        /// Original context supplied by the frontend.
        context: String,
        /// Original min length supplied by the frontend.
        min_length: u64,
        /// Original max length supplied by the frontend.
        max_length: u64,
    },
    /// An array schema specifies `minItems` greater than `maxItems`.
    InvalidArrayLengthBounds {
        /// Original context supplied by the frontend.
        context: String,
        /// Original min items supplied by the frontend.
        min_items: u64,
        /// Original max items supplied by the frontend.
        max_items: u64,
    },
    /// A schema uses a keyword that is not safely supported.
    UnsupportedKeyword {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: String,
    },
    /// A schema keyword that must be a non-negative integer has an invalid value.
    InvalidNonNegativeIntegerKeyword {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
    },
    /// An `exclusiveMinimum`/`exclusiveMaximum` keyword is present but the corresponding bound is missing.
    ExclusiveLimitRequiresBound {
        /// Original context supplied by the frontend.
        context: String,
        /// Original exclusive keyword supplied by the frontend.
        exclusive_keyword: &'static str,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
    },
    /// A schema keyword that must be a finite number has a non-finite value.
    InvalidFiniteNumberKeyword {
        /// Original context supplied by the frontend.
        context: String,
        /// Original keyword supplied by the frontend.
        keyword: &'static str,
    },
    /// A value expected to be an integer is not.
    ExpectedInteger {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// Integer bounds (minimum/maximum) do not permit any value.
    EmptyIntegerBounds {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// Number bounds (minimum/maximum) do not permit any value.
    EmptyNumberBounds {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// An operation does not declare any responses.
    MissingOperationResponses {
        /// Original operation id supplied by the frontend.
        operation_id: String,
    },
    /// A value expected to be an array is not.
    ExpectedArray {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A parameter uses an unsupported location (e.g. cookie) instead of path, query, or header.
    UnsupportedParameterLocation {
        /// Original context supplied by the frontend.
        context: String,
        /// Original wire name supplied by the frontend.
        wire_name: String,
        /// Original location supplied by the frontend.
        location: String,
    },
    /// A parameter uses `content` instead of `schema`.
    ContentParameterUnsupported {
        /// Original context supplied by the frontend.
        context: String,
        /// Original wire name supplied by the frontend.
        wire_name: String,
    },
    /// A parameter is missing a required `schema` declaration.
    MissingParameterSchema {
        /// Original context supplied by the frontend.
        context: String,
        /// Original wire name supplied by the frontend.
        wire_name: String,
    },
    /// A parameter is nullable, which is not supported.
    NullableParameterUnsupported {
        /// Original wire name supplied by the frontend.
        wire_name: String,
    },
    /// A path parameter does not set `required: true`.
    PathParameterNotRequired {
        /// Original wire name supplied by the frontend.
        wire_name: String,
    },
    /// A context is missing a required `content` declaration.
    MissingContent {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A context is missing the required `application/json` content type.
    MissingJsonContent {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A context's `application/json` content is missing a schema.
    MissingJsonSchema {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A response body uses the `default` status, which is not yet supported for decoding.
    DefaultResponseBodyUnsupported {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A response contains an invalid HTTP status code string.
    InvalidStatusCode {
        /// Original context supplied by the frontend.
        context: String,
        /// Original status supplied by the frontend.
        status: String,
    },
    /// A response contains a status code outside the valid 100–599 range.
    OutOfRangeStatusCode {
        /// Original context supplied by the frontend.
        context: String,
        /// Original status code supplied by the frontend.
        status_code: u16,
    },
    /// A response for a given status code is missing `application/json` content.
    MissingResponseJsonContent {
        /// Original context supplied by the frontend.
        context: String,
        /// Original status supplied by the frontend.
        status: String,
    },
    /// `x-satay.output` was configured on an operation with no JSON response body.
    SatayOutputRequiresResponseBody {
        /// Original operation id supplied by the frontend.
        operation_id: String,
    },
    /// A response projection selector was applied to a schema that is not an object with fields.
    SatayOutputExpectedObject {
        /// Original context supplied by the frontend.
        context: String,
        /// Original selector supplied by the frontend.
        selector: &'static str,
    },
    /// A response projection selector names a field absent from its object schema.
    UnknownSatayOutputField {
        /// Original context supplied by the frontend.
        context: String,
        /// Original selector supplied by the frontend.
        selector: &'static str,
        /// Original field supplied by the frontend.
        field: String,
    },
    /// `x-satay.output.map-field` follows an unwrapped value that is not an array.
    SatayOutputMapRequiresArray {
        /// Original context supplied by the frontend.
        context: String,
        /// Original field supplied by the frontend.
        field: String,
    },
    /// A path template contains a parameter that is never closed.
    UnclosedPathParameter {
        /// Original path supplied by the frontend.
        path: String,
    },
    /// A path template contains an empty parameter (e.g. `{}`).
    EmptyPathParameter {
        /// Original path supplied by the frontend.
        path: String,
    },
    /// A path template references a parameter that is not declared in the operation's parameters.
    UndeclaredPathParameter {
        /// Original path supplied by the frontend.
        path: String,
        /// Original name supplied by the frontend.
        name: String,
    },
    /// A parameter is declared for a path but never used in the path template.
    UnusedPathParameter {
        /// Original path supplied by the frontend.
        path: String,
        /// Original name supplied by the frontend.
        name: String,
    },
    /// A `$ref` could not be resolved because the referenced component failed validation.
    ResolveReference {
        /// Original reference supplied by the frontend.
        reference: String,
        /// Original context supplied by the frontend.
        context: String,
        /// Original source supplied by the frontend.
        source: Box<crate::Diagnostic>,
    },
    /// A reference points to an external document; only local (`#`) references are supported.
    NonLocalReference,
    /// A local reference is not a valid JSON pointer.
    InvalidLocalReference,
    /// A JSON pointer is missing a required token segment.
    MissingJsonPointerToken {
        /// Original token supplied by the frontend.
        token: String,
    },
    /// A `$ref` does not point to the expected `#/components/{section}/…` path.
    InvalidComponentReference {
        /// Original reference supplied by the frontend.
        reference: String,
        /// Original section supplied by the frontend.
        section: &'static str,
    },
    /// A local `$ref` chain references itself.
    CircularReference {
        /// Original reference supplied by the frontend.
        reference: String,
    },
    /// A value expected to be an object is not.
    ExpectedObject {
        /// Original context supplied by the frontend.
        context: String,
    },
    /// A failure constructing a checked interpretation.
    Interpretation {
        /// Original location supplied by the frontend.
        location: crate::SourceRef,
        /// Original source supplied by the frontend.
        source: crate::InterpretationError,
    },
    /// A retained media schema references an excluded definition.
    ExcludedDefinition {
        /// Original name supplied by the frontend.
        name: String,
        /// Original location supplied by the frontend.
        location: crate::SourceRef,
    },
    /// An API key declares an unknown location.
    ApiKeyLocation {
        /// Original value supplied by the frontend.
        value: String,
        /// Original location supplied by the frontend.
        location: crate::SourceRef,
    },
}
