//! Backend error types for Rust client generation.

use satay_ir::Diagnostic;

pub use crate::lower::error::ValidationError;

/// All errors that can occur while generating Rust code from a semantic IR.
///
/// This enum distinguishes Rust-support validation failures raised by this
/// backend from semantic diagnostics that were retained inside the input
/// graph. The latter is not a dependency on the OpenAPI frontend: it only
/// means the caller passed a graph that still carries a deferred diagnostic.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The semantic graph uses a construct the generated Rust types cannot
    /// represent.
    #[error(transparent)]
    Rust(#[from] ValidationError),

    /// The graph retains a semantic diagnostic that is surfaced unchanged.
    #[error(transparent)]
    Frontend(#[from] Diagnostic),
}
