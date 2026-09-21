# satay-codegen

Generate Rust client code from an OpenAPI 3.1 document.

This is a library crate and the OpenAPI-facing facade for code generation. It parses an OpenAPI spec, normalizes it into an owned `satay-ir` semantic graph, and hands that graph to `satay-codegen-rust`, which validates Rust representability, lowers, and renders the client sources: structs, enums, constrained newtypes, request builders, and response decoders. The output is a `Vec<GeneratedFile>` that can be written to disk or processed further.

For a command-line interface, use `satay-cli`.

```rust
use satay_codegen::{generate, GenerateOptions, RootModule};

let files = generate(openapi_yaml)?;
// or with options:
let files = generate_with(openapi_yaml, GenerateOptions {
    root_module: RootModule::LibRs,
})?;
```

## Facade role

`satay-codegen` owns three things and delegates the rest:

- **Parsing and reference resolution.** The spec string is parsed with the in-tree `satay-oas3` fork of `oas3`, and supported local references are validated early.
- **Normalization.** `parse::normalize` converts the resolved document into an owned `satay_ir::Api`. It preserves source-name identities, ordered compositions, declared constraints and interpretation hints, all HTTP media/schema associations, response projections, and source locations. The production entry retains recoverable failures inside the graph as `satay_ir::Diagnostic` values so backend validation can order them against Rust-policy failures; unsupported alternative-media schemas fail explicitly.
- **Error compatibility.** Backend `Error::Rust` rejections convert field-for-field into the legacy `satay_codegen::ValidationError`, and retained `Error::Frontend` diagnostics are restored through `parse::diagnostic::try_restore`; a diagnostic kind with no legacy shape becomes `Error::Internal`.

`GenerateOptions`, `RootModule`, and `GeneratedFile` are re-exported from `satay-codegen-rust`, so the public API stays identical whether you start from OpenAPI or from a graph.

The frontend recovers explicit null defaults and absent-versus-empty local server/security lists with a sparse source-presence reader. It is not a lossless OpenAPI document model: other distinctions erased by the typed parser, including empty composition arrays and `const: null`, are not recovered. Successful semantic normalization is not a guarantee of Rust representability; the backend can still reject a graph that the generated Rust types cannot represent.

## Direct IR-to-Rust usage

If you already have a `satay_ir::Api` — built with `satay_ir::ApiBuilder` or produced by any other frontend — call the backend directly instead:

```rust
use satay_codegen_rust::{generate, GenerateOptions};

let files = generate(&api, GenerateOptions::default())?;
```

See the [`satay-codegen-rust` README](../satay-codegen-rust/README.md) for the backend API and its error model.

## Testing the semantic frontend

Run the owned-graph smoke scenario with:

```sh
cargo test -p satay-codegen --lib parse::tests::ir::owned_frontend_end_to_end --locked -- --exact
```

If you are looking for `satay-rs`, visit the [main repository](https://github.com/zeon256/satay-rs).
