#![forbid(unsafe_code)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::doc_markdown,
    clippy::elidable_lifetime_names,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::needless_pass_by_value,
    clippy::ref_option,
    clippy::redundant_closure_for_method_calls,
    clippy::single_match_else,
    clippy::struct_field_names,
    clippy::trivially_copy_pass_by_ref,
    clippy::unnecessary_wraps
)]

//! Rust client-code backend for Satay.
//!
//! This crate consumes a finalized [`satay_ir::Api`] semantic graph and
//! produces Rust client sources. It performs its own validation and lowering:
//! a semantic graph that is final with respect to the IR is not necessarily
//! representable by the generated Rust types, so [`generate`] remains
//! fallible.
//!
//! The backend is IO-free and parser-independent: it never parses OpenAPI
//! documents and never touches file systems. Callers normalize their
//! documents into a semantic IR first (for example with `satay-codegen`),
//! then pass the owned graph here.

mod error;
mod ident;
mod lower;
mod model;
mod render;

pub use error::Error;
pub use lower::error::ValidationError;
pub use render::GeneratedFile;
use tracing::info;

/// Which root module file to emit at the output directory root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RootModule {
    /// `mod.rs` (default).
    #[default]
    ModRs,
    /// `lib.rs` for a generated crate root.
    LibRs,
}

/// Options for Rust client code generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GenerateOptions {
    /// Root module filename (`mod.rs` or `lib.rs`).
    pub root_module: RootModule,
}

/// Validates, lowers, and renders Rust client files from a semantic IR graph.
///
/// The graph is consumed by reference; all frontend state may be dropped
/// before calling this function. Rust-unsupported constructs are rejected
/// with structured [`ValidationError`] payloads; semantic diagnostics
/// retained in the graph are surfaced through [`Error::Frontend`].
///
/// # Errors
///
/// Returns an error if the graph cannot be lowered to Rust, or carries a
/// retained semantic diagnostic.
#[tracing::instrument(err)]
pub fn generate(
    api: &satay_ir::Api,
    options: GenerateOptions,
) -> Result<Vec<GeneratedFile>, Error> {
    let model = lower::lower_model(api)?;
    info!(
        components = model.components.len(),
        operations = model.operations.len(),
        "lowered semantic IR"
    );
    Ok(render::render_api(&model, options))
}
