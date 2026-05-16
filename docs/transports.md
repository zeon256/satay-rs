# Transports

Satay's core contract is Sans-IO: generated actions build `http::Request<Vec<u8>>` values and decode `satay_runtime::ResponseParts<B>`. The optional adapter crates only add small extension traits for common clients.

## Action Builders

Action builders handle request construction and response decoding without IO, with less boilerplate than calling the `_parts` functions directly:

```rust
let api = generated::Api::new()
    .account_key(std::env::var("LTA_ACCOUNT_KEY")?);

let request = api
    .get_bus_arrival("83139")
    .request()?;

// Send `request` with reqwest, ureq, hyper, tests, WASM, or your own transport.

let response = satay_runtime::ResponseParts { status, headers, body };
let decoded = generated::GetBusArrivalAction::decode(response)?;
```

To compile JSON request and response helpers, define the generated crate's `json` feature:

```toml
[features]
default = ["serde", "json"]
serde = ["dep:serde", "satay-runtime/serde"]
json = ["serde", "dep:serde_json", "satay-runtime/json"]
```

## Adapter Crates

With `satay-reqwest`, simple call sites can stay compact:

```rust
use satay_reqwest::{ReqwestActionExt, reqwest};

let client = reqwest::Client::new();
let response = api.get_bus_arrival("83139").send_with(&client).await?;
```

If your application needs transport features such as blocking support, proxies, or custom client configuration, keep a direct dependency on the transport crate and enable those features there. Reqwest 0.13 uses rustls by default, so simple HTTPS clients do not need an explicit TLS feature:

```toml
[dependencies]
satay-reqwest = "0.1"
reqwest = "0.13.3"
```

The adapter crate also depends on `reqwest` so it can name `reqwest::Client` in its extension trait. Cargo unifies compatible dependency versions, so this normally selects one shared `reqwest` build rather than two copies. The same model applies to `satay-ureq`: use the adapter for `.send_with(&agent)`, and let your application own the `ureq` configuration.

## Manual Transport

For the lower-level path without an adapter crate, see `examples/reqwest-manual`. It calls `action.request()`, sends the request with `reqwest`, builds `satay_runtime::ResponseParts`, and calls the generated decoder directly.

```rust
let request: reqwest::Request = action.request()?.try_into()?;
let mut response = reqwest::Client::new().execute(request).await?;

let response = satay_runtime::ResponseParts {
    status: response.status(),
    headers: std::mem::take(response.headers_mut()),
    body: response.bytes().await?,
};

let decoded = generated::GetBusArrivalAction::decode(response)?;
```

## Non-HTTP Transports

Generated requests are HTTP-shaped data, but they do not have to be sent by an HTTP client. `examples/tungstenite-ws` sends a generated action over a local WebSocket using `tokio-tungstenite`.

A custom transport needs to preserve the parts Satay cares about:

- request method
- request URI
- request headers
- request body
- response status
- response headers
- response body

The WebSocket example adds a small wire protocol and an extension trait so call sites can use `.send_over_ws(&mut transport).await` instead of manually calling `request()`, sending the request, and calling `decode(...)`.
