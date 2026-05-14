# Satay

<p align="center">
  <img src="logo.png" alt="satay-rs logo" width="300">
  <br>
  <sub><i>Disclaimer: I can't design. This logo was generated using ChatGPT.</i></sub>
</p>

Satay generates typed Rust OpenAPI clients without choosing your HTTP client.

Satay is Sans-IO by design. Generated code builds HTTP requests and decodes HTTP responses, but never sends bytes over the network. Bring your own transport: `reqwest`, `ureq`, `hyper`, tests, WASM, or custom runtime code.

```bash
satay generate --input openapi.yaml --output src/generated --rustfmt
```

If `--output` ends in `.rs`, Satay writes that file. Otherwise it writes `mod.rs` inside the output directory.

## Current support

Satay currently targets OpenAPI 3.0.x and a deliberately small, typed subset.

- YAML or JSON OpenAPI documents.
- `components.schemas` as Rust structs, string enums, primitive aliases, and constrained newtypes.
- Schema types: `string`, `integer` (`int32`, `int64`, or no format), `number` (`float`, `double`, or no format), `boolean`, arrays, nullable values, and local `#/components/schemas/...` references.
- Operations for standard HTTP methods with explicit `operationId`, or inferred names from method + path.
- Path and query parameters declared with `schema`.
- Path-level parameters with operation-level overrides.
- JSON request bodies using `application/json` or structured JSON media types such as `application/problem+json`.
- Generated `<operation>_parts` functions that return `satay_runtime::RequestParts<B>` without requiring an HTTP client.
- Generated `encode_<operation>` helpers, behind the generated crate's `json` feature, that produce `http::Request<Vec<u8>>`.
- Generated `decode_<operation>_response` helpers, behind the generated crate's `json` feature, that decode known JSON responses and preserve unknown statuses as `UnexpectedStatus(http::StatusCode, Vec<u8>)`.
- Percent-encoded path and query values.
- Repeated query parameters for array query values.
- Optional fields and optional request bodies.
- `serde` derives and field renames behind the generated crate's `serde` feature.
- Validation constraints rendered through `nutype` for:
  - string `minLength` / `maxLength`
  - string `pattern` (via `nutype`'s `regex` validator)
  - integer and number `minimum` / `maximum` with `exclusiveMinimum` / `exclusiveMaximum`
  - array `minItems` / `maxItems`

Generated code that represents OpenAPI validation constraints uses `nutype` newtypes. Add these dependencies to crates that compile constrained generated clients:

```toml
nutype = { version = "0.7", features = ["serde"] }
```

When the OpenAPI spec contains string `pattern` constraints, also add:

```toml
nutype = { version = "0.7", features = ["serde", "regex"] }
regex = "1"
```

> **Note:** OpenAPI `pattern` uses ECMA-262 (JavaScript) regex syntax, while `nutype` uses the Rust `regex` crate. Most common patterns (character classes, quantifiers, anchors) are compatible. However, ECMA features like lookahead, lookbehind, and backreferences are not supported by the Rust `regex` engine and will cause a compile error in the generated code.

## Not supported yet

These are known gaps rather than silent compatibility promises:

- OpenAPI 3.1.
- Remote or file `$ref`s.
- Built-in HTTP transport, retries, authentication, or server/base-URL selection.
- Header and cookie parameters.
- `content` parameters; parameters must use `schema`.
- Nullable parameters.
- Array path parameters and OpenAPI parameter style/explode variants beyond repeated query pairs.
- Non-JSON request or response bodies, including form, multipart, and raw byte bodies.
- Default response bodies.
- Inline object schemas outside `components.schemas`.
- Map schemas / `additionalProperties`.
- `oneOf`, `anyOf`, `allOf`, and discriminator-based polymorphism.
- Non-string enums.
- Numeric `multipleOf`.
- Array `uniqueItems`.
- Object `minProperties` / `maxProperties`.

## Roadmap

- Broaden schema coverage: maps, inline objects, composition, and discriminators.
- Broaden parameter support: header/cookie parameters and OpenAPI style/explode encoding.
- Broaden body support beyond JSON: form data, multipart, and bytes.
- Improve reference handling with validated schema references and remote reference loading.
- Add OpenAPI 3.1 support once the schema subset is explicit and tested.
- Add first-class examples for common transports while keeping generated clients Sans-IO.

## Workspace

- `crates/satay-cli`: user-facing `satay` executable.
- `crates/satay-codegen`: OpenAPI parser, normalized IR, and Rust generator.
- `crates/satay-runtime`: small IO-free support crate for generated code.
