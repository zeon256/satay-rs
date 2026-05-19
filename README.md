# Satay

[![Crates.io](https://img.shields.io/crates/v/satay)](https://crates.io/crates/satay)
[![Crates.io Downloads](https://img.shields.io/crates/d/satay)](https://crates.io/crates/satay)
[![Docs.rs](https://img.shields.io/docsrs/satay)](https://docs.rs/satay)
[![License](https://img.shields.io/badge/license-Apache--2.0%2FMIT-blue)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.88.0-orange)](https://blog.rust-lang.org/2025/02/20/Rust-1.88.0/)
[![Rust Edition](https://img.shields.io/badge/Rust-2024-blue)](https://doc.rust-lang.org/edition-guide/rust-2024/)

satay is a Rust OpenAPI client generator for [sans-io](https://fasterthanli.me/articles/the-case-for-sans-io) clients. It generates typed request builders, response decoders, and validation [newtypes](https://rust-unofficial.github.io/patterns/patterns/behavioural/newtype.html) from OpenAPI constraints while leaving HTTP, WebSocket, tests, WASM, and other transports under your application's control.

<p align="center">
  <img src="logo.png" alt="satay-rs logo" width="180">
</p>

> [!WARNING]
> Satay is work in progress and currently prioritizes generating `lta-rs` clients with good ergonomics and validation. Support for other OpenAPI specs will grow as the supported subset becomes explicit and tested.

## Features

- Generates from OpenAPI 3.0 documents
- Sans-IO design from the ground up, with optional transport adapters for `reqwest` and `ureq`
- Validation newtypes for OpenAPI string, number, integer, and array constraints
- Automatic number type deduction from specified bounds (i.e. if `maximum` is less than `u8::MAX`, the generated type will be a `u8` newtype instead of `u64`)

## Quick Start

Install the CLI from a checkout of this repository:

```bash
cargo install --path crates/satay-cli
satay --help
```

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
- Satay extensions under `x-satay` for typed string parsing, integer primitive overrides, enum variant names, and lossy optional fields.

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

## Security

Please see [SECURITY.md](SECURITY.md) for the security policy, reporting guidelines, and hardening techniques used in this project.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.


## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
