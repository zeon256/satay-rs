# Satay

<p align="center">
  <img src="logo.png" alt="satay-rs logo" width="180">
</p>

Satay is a Rust OpenAPI client generator for Sans-IO clients. It generates typed request builders, response decoders, and validation newtypes from OpenAPI constraints while leaving HTTP, WebSocket, tests, WASM, and other transports under your application's control.

> [!WARNING]
> Satay is work in progress and currently prioritizes generating `lta-rs` clients with good ergonomics and validation. Support for other OpenAPI specs will grow as the supported subset becomes explicit and tested.

## Quick Start

Generate Rust code from an OpenAPI document:

```bash
satay generate --input openapi.yaml --output src/generated --rustfmt
```

If `--output` ends in `.rs`, Satay writes that file. Otherwise it writes `mod.rs` inside the output directory.

Then use the generated action API with whichever transport you want. With the `satay-reqwest` adapter, the call site can stay compact:

```rust
include!(concat!(env!("OUT_DIR"), "/satay_generated.rs"));

use generated::{Api, GetBusArrivalResponse};
use satay_reqwest::{ReqwestActionExt, reqwest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = Api::new()
        .account_key(std::env::var("LTA_ACCOUNT_KEY")?);

    let client = reqwest::Client::new();
    let response = api
        .get_bus_arrival("83139")
        .service_no("15")
        .send_with(&client)
        .await?;

    match response {
        GetBusArrivalResponse::Ok(arrival) => {
            println!(
                "{} services for bus stop {}",
                arrival.services.len(),
                arrival.bus_stop_code
            );
        }
        GetBusArrivalResponse::UnexpectedStatus(status, body) => {
            eprintln!("unexpected status {status}: {}", String::from_utf8_lossy(&body));
        }
    }

    Ok(())
}
```

The adapter is optional. Generated actions expose the same IO-free boundary directly:

```rust
let request: http::Request<Vec<u8>> = action.request()?;

// Send `request` with reqwest, ureq, hyper, tests, WASM, WebSocket, or your own transport.

let response = satay_runtime::ResponseParts { status, headers, body };
let decoded = generated::GetBusArrivalAction::decode(response)?;
```

## What It Generates

- Rust structs, string enums, primitive aliases, and constrained newtypes from `components.schemas`.
- Operation input builders with required constructor arguments and chainable optional setters.
- Request builders that return `http::Request<Vec<u8>>` without choosing a transport.
- Response decoders for known JSON responses, preserving unknown statuses as `UnexpectedStatus(http::StatusCode, Vec<u8>)`.
- Optional `Api` action builders for base URL, API-key auth, request construction, and response decoding.
- `serde` derives and field renames behind a generated crate `serde` feature.
- `nutype` validation for OpenAPI string, number, integer, and array bounds.
- Satay extensions under `x-satay` for typed string parsing, enum variant names, and lossy optional fields.

Generated code that represents OpenAPI validation constraints uses `nutype` newtypes. Add these dependencies to crates that compile constrained generated clients:

```toml
nutype = { version = "0.7", features = ["serde"] }
```

When the OpenAPI spec contains string `pattern` constraints, also add:

```toml
nutype = { version = "0.7", features = ["serde", "regex"] }
regex = "1"
```

OpenAPI `pattern` uses ECMA-262 regex syntax, while `nutype` uses Rust's `regex` crate. Common patterns are usually compatible, but JavaScript-only features such as lookahead, lookbehind, and backreferences will not compile in generated Rust code.

## Docs

- [Supported OpenAPI subset](docs/support.md)
- [Generated validation newtypes](docs/validation.md)
- [Satay extensions](docs/extensions.md)
- [Action builders and transport adapters](docs/transports.md)

## Examples

- [examples/reqwest](examples/reqwest): generates from `examples/openapi.yaml` at build time, sends the request with `reqwest`, and decodes with the generated action API.
- [examples/reqwest-blocking](examples/reqwest-blocking): uses the same generated action API with `reqwest::blocking`.
- [examples/reqwest-manual](examples/reqwest-manual): sends with `reqwest` directly without using `satay-reqwest`.
- [examples/tungstenite-ws](examples/tungstenite-ws): sends generated actions over a local WebSocket using `tokio-tungstenite`.
- [examples/ureq](examples/ureq): sends generated actions with `ureq`.

## Workspace

- [crates/satay-cli](crates/satay-cli): user-facing `satay` executable.
- [crates/satay-codegen](crates/satay-codegen): OpenAPI parser, normalized IR, and Rust generator.
- [crates/satay-runtime](crates/satay-runtime): small IO-free support crate for generated code.
- [crates/satay-reqwest](crates/satay-reqwest): adapter traits for sending generated actions with `reqwest`.
- [crates/satay-ureq](crates/satay-ureq): adapter traits for sending generated actions with `ureq`.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
