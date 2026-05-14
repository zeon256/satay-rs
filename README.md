# Satay

Satay generates typed OpenAPI clients without choosing your HTTP client.

Satay is Sans-IO by design. Generated code builds HTTP requests and decodes HTTP responses, but never sends bytes over the network. Bring your own transport: `reqwest`, `ureq`, `hyper`, tests, mocks, WASM, or custom runtime code.

```bash
satay generate --input openapi.yaml --output src/generated
```

Generated code that represents OpenAPI validation constraints uses `nutype` newtypes. Add this dependency to crates that compile constrained generated clients:

```toml
nutype = { version = "0.7", features = ["serde"] }
```

## Workspace

- `crates/satay-cli`: user-facing `satay` executable.
- `crates/satay-codegen`: OpenAPI parser, normalized IR, and Rust generator.
- `crates/satay-runtime`: small IO-free support crate for generated code.
