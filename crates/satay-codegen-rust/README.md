# satay-codegen-rust

Rust client-code backend for [Satay](https://github.com/zeon256/satay-rs).
Consumes a finalized [`satay-ir`](https://docs.rs/satay-ir) semantic graph and
produces Rust client sources; no OpenAPI parsing, no file IO.

## Direct IR-to-Rust usage

```rust
use satay_codegen_rust::{generate, GenerateOptions};

fn emit(api: &satay_ir::Api) -> Result<Vec<satay_codegen_rust::GeneratedFile>, satay_codegen_rust::Error> {
    generate(api, GenerateOptions::default())
}
```

Build the `satay_ir::Api` with `satay_ir::ApiBuilder` or normalize an OpenAPI
document with `satay-codegen`, then call [`generate`]. Generation validates
Rust representability, lowers the graph, and renders files in a deterministic
order; writing files and formatting are the caller's responsibility.

## Errors

- `Error::Rust`: the graph uses a construct the generated Rust types cannot
  represent; payloads are structured and parser-independent.
- `Error::Frontend`: the graph still carries a retained semantic diagnostic.

`satay-codegen` wraps this crate: it parses OpenAPI 3.1 documents, normalizes
them into the semantic IR, and translates backend errors into its public
diagnostics.
