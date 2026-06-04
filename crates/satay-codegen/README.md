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

If you are looking for `satay-rs`, visit the [main repository](https://github.com/zeon256/satay-rs).
