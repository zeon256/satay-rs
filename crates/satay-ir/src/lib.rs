//! Resolved semantic schema graphs for Satay.
//!
//! This crate is the first semantic-IR migration boundary. It models schema
//! definitions and uses without carrying parser state or committing to Rust
//! names and representations. HTTP operations, schema compositions, and typed
//! Satay interpretation hints are intentionally outside this first contract.
//!
//! A [`Definition`] owns its root [`SchemaUse`]. Every inline array item,
//! object property, and typed additional-property schema is another owned use;
//! a [`TypeExpr::Ref`] stores only a [`DefinitionId`]. Annotations therefore
//! remain attached to the exact use where they appeared.
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
mod error;
mod schema;
mod source;

pub use api::Api;
pub use builder::ApiBuilder;
pub use error::{BuildError, BuildErrors};
pub use schema::{
    AdditionalProperties, ArrayConstraints, ArraySchema, Definition, DefinitionId, IntegerSchema,
    NumberSchema, NumericBound, NumericConstraints, ObjectSchema, Property, SchemaAnnotations,
    SchemaUse, StringConstraints, StringSchema, TypeExpr,
};
pub use source::SourceRef;
