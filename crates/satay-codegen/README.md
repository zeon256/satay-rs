# satay-codegen

Generate Rust client code from an OpenAPI 3.1 document.

This is a library crate. It parses an OpenAPI spec into an internal IR and renders Rust structs, enums, constrained newtypes, request builders, and response decoders. The output is a `Vec<GeneratedFile>` that can be written to disk or processed further.

For a command-line interface, use `satay-cli`.

```rust
use satay_codegen::{generate, GenerateOptions, RootModule};

let files = generate(openapi_yaml)?;
// or with options:
let files = generate_with(openapi_yaml, GenerateOptions {
    root_module: RootModule::LibRs,
})?;
```

## Semantic frontend staging

A private `cfg(test)` frontend normalizes OpenAPI into an owned `satay-ir::Api`.
It preserves source-name identities, ordered compositions, declared constraints
and interpretation hints, all HTTP media/schema associations, response projections,
and source locations. Unsupported alternative-media schemas fail explicitly.
`generate`, `generate_with`, and production parsing still use the existing pipeline;
successful semantic normalization is not a guarantee of Rust representability.

The frontend recovers explicit null defaults and absent-versus-empty local
server/security lists with a sparse source-presence reader. It is not a lossless
OpenAPI document model: other distinctions erased by the typed parser, including
empty composition arrays and `const: null`, are not recovered.

Run the owned-graph smoke scenario with:

```sh
cargo test -p satay-codegen --lib parse::tests::ir::owned_frontend_end_to_end --locked -- --exact
```

If you are looking for `satay-rs`, visit the [main repository](https://github.com/zeon256/satay-rs).
