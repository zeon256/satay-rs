//! Compatibility translation for deferred semantic diagnostics.
use crate::ValidationError;
use satay_ir::{Diagnostic, DiagnosticKind};
use serde::de::Error as _;

macro_rules! diagnostic_mapping {
    ($($variant:ident $( { $($field:ident),* } )?),* $(,)?) => {
        #[allow(clippy::clone_on_copy)] // The mapping includes owned and Copy payloads.
        pub(super) fn retain(error: &ValidationError) -> Diagnostic {
            let kind = match error {
                $(ValidationError::$variant $( { $($field),* } )? =>
                    DiagnosticKind::$variant $( { $($field: $field.clone()),* } )?,)*
                ValidationError::InvalidExtension { context, path, source } => {
                    // Extensions deserialize from a Value: these are data errors
                    // without parser line/column coordinates.
                    assert!(source.is_data() && source.line() == 0 && source.column() == 0);
                    DiagnosticKind::InvalidExtension { context: context.clone(), path: path.clone(), source: source.to_string() }
                }
                ValidationError::ResolveReference { reference, context, source } => DiagnosticKind::ResolveReference {
                    reference: reference.clone(), context: context.clone(), source: Box::new(retain(source)),
                },
                error => panic!("target-only error deferred by frontend: {error:?}"),
            };
            Diagnostic { kind, message: error.to_string() }
        }
        pub(super) fn try_restore(diagnostic: Diagnostic) -> Result<ValidationError, DiagnosticKind> {
            Ok(match diagnostic.kind {
                $(DiagnosticKind::$variant $( { $($field),* } )? =>
                    ValidationError::$variant $( { $($field),* } )?,)*
                DiagnosticKind::InvalidExtension { context, path, source } => ValidationError::InvalidExtension {
                    context, path, source: serde_json::Error::custom(source),
                },
                DiagnosticKind::ResolveReference { reference, context, source } => ValidationError::ResolveReference {
                    reference, context, source: Box::new(try_restore(*source)?),
                },
                kind => return Err(kind),
            })
        }
    };
}

diagnostic_mapping! {
    UnsupportedOpenApiVersion { version },
    UnsupportedComponentType { schema, kind },
    MissingComponentSchemaType { schema },
    MissingObjectProperties { schema },
    MissingPaths,
    UnsupportedEnumType { context, kind },
    NonArrayEnum { context },
    EmptyEnum { context },
    NonStringEnumValue { context },
    ConstNotInEnum { context },
    SatayEnumVariantsRequireEnum { context },
    SatayParseAsWithEnum { context, parse_as },
    SatayOptionUnsupportedWithEnum { context, keyword },
    InvalidSatayCoordinates { context, reason },
    SatayOptionRequiresCoordinates { context, keyword },
    UnknownSatayEnumVariantValue { context, wire_name },
    NonArrayRequired { context },
    NonStringRequiredField { context },
    UnsupportedNumberFormat { context, format },
    SatayParseAsRequiresString { context, parse_as, kind },
    SatayParseAsBoolWithIntegerType { context, integer_type },
    EmptySatayNoneIf { context },
    SatayNoneIfRequiresStructField { context },
    SatayNoneIfRequiresParsedString { context },
    ConflictingSatayNoneHandling { context },
    SatayBoolMappingRequiresParsedStringBool { context },
    IncompleteSatayBoolMapping { context },
    EmptySatayBoolMapping { context, keyword },
    OverlappingSatayBoolMapping { context, value },
    OverlappingSatayBoolMappingNoneIf { context, value },
    SatayTreatErrorAsNoneRequiresObjectProperty { context },
    SatayIgnoreRequiresObjectProperty { context },
    SatayOptionConflictsWithIgnore { context, keyword },
    SatayIdentifierRequiresObjectProperty { context },
    SatayIntegerTypeRequiresInteger { context, integer_type, kind },
    MissingArrayItems { context },
    InlineObjectSchema { context },
    UnsupportedMapObjectSchema { context },
    UnsupportedSchemaType { context, kind },
    MissingSchemaType { context },
    UnsupportedBooleanSchema { context },
    MultipleNonNullSchemaTypesUnsupported { context },
    UnsupportedComposition { context, keyword },
    UnsupportedRefSiblingKeyword { context, keyword },
    UnsupportedAllOfSiblingKeyword { context, keyword },
    UnsupportedAllOfBranch { context, index },
    DuplicateAllOfProperty { context, property },
    RecursiveAllOf { context, schema },
    RecursiveDiscriminatorBranch { context, schema },
    UnsupportedAnyOfSiblingKeyword { context, keyword },
    UnsupportedAnyOfBranch { context, index },
    UnsupportedOneOfSiblingKeyword { context, keyword },
    UnsupportedOneOfBranch { context, index },
    DuplicateUnionNullBranch { context, keyword, index },
    DuplicateOpenStringEnumValue { context, value },
    NullableUnionWithoutVariants { context, keyword },
    ShadowedUnionBranch { context, keyword, index, shadowed_by },
    EmptyAnyOf { context },
    RecursiveAnyOf { context, schema },
    InvalidDiscriminatorUnion { context },
    UnsupportedDiscriminatorBranch { context, keyword, index },
    DiscriminatorBranchNotObject { context, schema },
    DiscriminatorPropertyConflict { context, schema, property },
    InvalidDiscriminatorProperty { context, schema, property, expected },
    InvalidDiscriminatorMapping { context, value, target },
    DuplicateDiscriminatorMapping { context, schema },
    DiscriminatorMappingValueMismatch { context, schema, value, actual },
    DuplicateDiscriminatorValue { context, value },
    InvalidStringLengthBounds { context, min_length, max_length },
    InvalidArrayLengthBounds { context, min_items, max_items },
    UnsupportedKeyword { context, keyword },
    InvalidNonNegativeIntegerKeyword { context, keyword },
    ExclusiveLimitRequiresBound { context, exclusive_keyword, keyword },
    InvalidFiniteNumberKeyword { context, keyword },
    ExpectedInteger { context },
    EmptyIntegerBounds { context },
    EmptyNumberBounds { context },
    MissingOperationResponses { operation_id },
    ExpectedArray { context },
    UnsupportedParameterLocation { context, wire_name, location },
    ContentParameterUnsupported { context, wire_name },
    MissingParameterSchema { context, wire_name },
    NullableParameterUnsupported { wire_name },
    PathParameterNotRequired { wire_name },
    MissingContent { context },
    MissingJsonContent { context },
    MissingJsonSchema { context },
    DefaultResponseBodyUnsupported { context },
    InvalidStatusCode { context, status },
    OutOfRangeStatusCode { context, status_code },
    MissingResponseJsonContent { context, status },
    SatayOutputRequiresResponseBody { operation_id },
    SatayOutputExpectedObject { context, selector },
    UnknownSatayOutputField { context, selector, field },
    SatayOutputMapRequiresArray { context, field },
    UnclosedPathParameter { path },
    EmptyPathParameter { path },
    UndeclaredPathParameter { path, name },
    UnusedPathParameter { path, name },
    NonLocalReference,
    InvalidLocalReference,
    MissingJsonPointerToken { token },
    InvalidComponentReference { reference, section },
    CircularReference { reference },
    ExpectedObject { context },
}

#[cfg(test)]
pub(super) fn restore(diagnostic: Diagnostic) -> ValidationError {
    try_restore(diagnostic).expect("test diagnostic belongs to compatibility contract")
}

#[cfg(test)]
mod tests;
