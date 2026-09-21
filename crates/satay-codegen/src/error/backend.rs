//! Typed conversion from backend Rust-lowering errors into the public
//! facade diagnostics.
//!
//! The backend owns a parser-independent validation error covering exactly the
//! cases Rust lowering can reject a semantic graph with. This conversion is
//! exhaustive over that backend enum and reconstructs every facade variant
//! field-for-field, preserving the public diagnostic payloads and messages.

use super::ValidationError;
use satay_codegen_rust as backend;

/// Field-for-field conversion table. Every backend variant maps to the
/// identically named facade variant; the exhaustive match keeps the
/// compatibility boundary honest when either side gains a variant.
macro_rules! backend_error_mapping {
    ($($variant:ident { $($field:ident),* }),* $(,)?) => {
        impl From<backend::ValidationError> for ValidationError {
            fn from(error: backend::ValidationError) -> Self {
                match error {
                    $(backend::ValidationError::$variant { $($field),* } => {
                        ValidationError::$variant { $($field),* }
                    })*
                }
            }
        }
    };
}

backend_error_mapping! {
    NonStringEnumValue { context },
    InvalidSatayCoordinates { context, reason },
    SatayCoordinatesRequireStructField { context },
    ReservedSatayEnumVariantName { context, wire_name, rust_name },
    DuplicateSatayEnumVariantName { context, rust_name },
    UnsupportedIntegerFormat { context, format },
    UnsupportedNumberFormat { context, format },
    SatayNoneIfRequiresParsedString { context },
    SatayTreatErrorAsNoneRequiresObjectProperty { context },
    DuplicateSatayIdentifierRustField { context, first_property, second_property, rust_name },
    InlineObjectSchema { context },
    UnsupportedMapObjectSchema { context },
    UnsupportedSchemaType { context, kind },
    UnsupportedComposition { context, keyword },
    UnsupportedAllOfBranch { context, index },
    DuplicateAllOfProperty { context, property },
    RecursiveAllOf { context, schema },
    RecursiveDiscriminatorBranch { context, schema },
    UnsupportedAnyOfBranch { context, index },
    UnsupportedOneOfBranch { context, index },
    DuplicateUnionNullBranch { context, keyword, index },
    DuplicateOpenStringEnumValue { context, value },
    NullableUnionWithoutVariants { context, keyword },
    ShadowedUnionBranch { context, keyword, index, shadowed_by },
    RecursiveAnyOf { context, schema },
    UnsupportedDiscriminatorBranch { context, keyword, index },
    DiscriminatorBranchNotObject { context, schema },
    InvalidDiscriminatorProperty { context, schema, property, expected },
    DuplicateDiscriminatorMapping { context, schema },
    DiscriminatorMappingValueMismatch { context, schema, value, actual },
    DuplicateDiscriminatorValue { context, value },
    InvalidStringLengthBounds { context, min_length, max_length },
    UniqueItemsUnsupported { context },
    InvalidArrayLengthBounds { context, min_items, max_items },
    UnsupportedKeyword { context, keyword },
    InvalidFiniteNumberKeyword { context, keyword },
    ExpectedInteger { context },
    EmptyIntegerBounds { context },
    ExclusiveIntegerMinimumOverflow { },
    ExclusiveIntegerMaximumOverflow { },
    EmptyNumberBounds { context },
    UnsupportedParameterLocation { context, wire_name, location },
    InvalidParameterDefault { wire_name, value, reason },
    NullableParameterUnsupported { wire_name },
    AnyOfParameterUnsupported { wire_name },
    MapParameterUnsupported { wire_name },
    ArrayPathParameterUnsupported { wire_name },
    ArrayHeaderParameterUnsupported { wire_name },
    MissingContent { context },
    MissingJsonContent { context },
    MissingJsonSchema { context },
    DefaultResponseBodyUnsupported { context },
    InvalidStatusCode { context, status },
    OutOfRangeStatusCode { context, status_code },
    OutOfRangeStatusClass { context, class },
    MappedResponseProjectionRequiresArray { context },
    MissingResponseJsonContent { context, status },
    SatayOutputRequiresResponseBody { operation_id },
    UnclosedPathParameter { path },
    UndeclaredPathParameter { path, name },
    UnusedPathParameter { path, name },
}
