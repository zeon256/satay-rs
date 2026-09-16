//! Resolved semantic schema graphs for Satay.
//!
//! This crate is a semantic-IR migration boundary. It models schema
//! definitions and uses without carrying parser state or committing to Rust
//! names and representations. Compositions, typed Satay interpretation
//! hints, and owned HTTP roots complete the semantic contract.
//!
//! A [`Definition`] owns its root [`SchemaUse`]. Every inline array item,
//! object property, and typed additional-property schema is another owned use;
//! a [`TypeExpr::Ref`] stores only a [`DefinitionId`]. Annotations therefore
//! remain attached to the exact use where they appeared. Local property
//! policies never mutate shared definitions.
//!
//! [`HttpApi`](crate::HttpApi) records paths, operations, security, and
//! metadata in caller order; finalization retains them without merging,
//! inferring, or filtering.
//!
//! Definition IDs belong to the builder/API mapping that issued them. Keep IDs
//! with their originating graph. An ID with the same raw arena index from a
//! different graph is not detected as foreign; cloning an [`Api`](crate::Api)
//! retains the original mapping.
//!
//! Finalization checks graph structure only. It is not a certificate that the
//! represented schemas are satisfiable, supported by a backend, or valid Rust.
//!
//! # Construction
//!
//! ```
//! use satay_ir::{ApiBuilder, Definition, SchemaUse, TypeExpr};
//!
//! let mut builder = ApiBuilder::new();
//! let user = builder.reserve_definition();
//! builder.define(
//!     user,
//!     Definition {
//!         source_name: "User".into(),
//!         schema: SchemaUse::new(TypeExpr::Boolean),
//!     },
//! )?;
//!
//! let api = builder.finish()?;
//! assert_eq!(api.definition(user).unwrap().source_name, "User");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]

mod api;
mod builder;
mod diagnostic;
mod error;
mod http;
mod interpretation;
mod schema;
mod security;
mod source;

pub use api::Api;
pub use builder::ApiBuilder;
pub use diagnostic::DiagnosticKind;
pub use error::{BuildError, BuildErrors, GraphOwner};
pub use http::{
    HttpApi, HttpMethod, MediaType, Operation, OperationInterpretation, OutputSelector, Parameter,
    ParameterLocation, ParameterStyle, PathItem, RequestBody, Response, ResponseMediaType,
    ResponseProjection, ResponseStatus, Server, ServerVariable, Tag,
};
pub use interpretation::{
    BoolMapping, CoordinatesInterpretation, DecodePolicy, EnumVariantName, IntegerInterpretation,
    IntegerRepresentation, InterpretationError, PropertyPolicy, SentinelValues,
    StringInterpretation, StringScalar,
};
pub use schema::{
    AdditionalProperties, ArrayConstraints, ArraySchema, CompositionKind, CompositionSchema,
    DeclaredNumericConstraints, Definition, DefinitionId, Diagnostic, Discriminator,
    DiscriminatorMapping, IntegerSchema, NumberSchema, NumericBound, NumericConstraints,
    ObjectSchema, Property, SchemaAnnotations, SchemaUse, StringConstraints, StringSchema,
    TypeExpr,
};
pub use security::{
    ApiKeyLocation, OAuthFlow, OAuthFlowKind, OAuthScope, SecurityRequirement,
    SecurityRequirementScheme, SecurityScheme, SecuritySchemeKind,
};
pub use source::SourceRef;
