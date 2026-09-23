# Transports

Satay's core contract is Sans-IO: `Action` chooses its `RequestBody` and exposes a GAT `Response<'de>`. Generated actions currently encode JSON into `Vec<u8>` and decode borrowed `ResponseParts<&'de [u8]>`. `send_with()` returns an owned decoded response through the `OwnedAction` contract. For explicit buffering and borrowed decoding, use `send_buffered_with()`; reqwest retains `Bytes`, and ureq retains `Vec<u8>`.

## Action Builders

Action builders handle request construction and response decoding without IO, with less boilerplate than calling the `_parts` functions directly:

```rust
let api = generated::Api::new()
    .account_key(std::env::var("LTA_ACCOUNT_KEY")?);

let request = api
    .bus().get_arrival(83139)
    .request()?;

// Send `request` with reqwest, ureq, hyper, tests, WASM, or your own transport.

let response = satay_runtime::ResponseParts { status, headers, body };
let decoded = generated::GetBusArrivalAction::<satay_runtime::storage::AllocStorage>::decode(response.as_bytes())?;
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
let decoded = api.bus().get_arrival(83139).send_with(&client).await?;
```

Generated actions implement both `Action` and `OwnedAction`. Custom borrowing-only
actions need only `Action` and can use the buffered path:

```rust
let response = action.send_buffered_with(&client).await?;
let decoded = response.decode()?; // May borrow from response.
```

The buffer also exposes status, headers, and bytes through `.parts()`. For actions
implementing `OwnedAction`, `.decode_owned()` consumes the buffer and returns an
independent model. `send_with()` performs this step automatically, including
reporting decode errors. Blocking reqwest and ureq expose the same two methods.

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

let decoded = generated::GetBusArrivalAction::<satay_runtime::storage::AllocStorage>::decode(response.as_bytes())?;
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

## Storage families

Generated text and arrays use `AllocStorage` by default (String and Vec).
Select boxed text and collections with
`Api::new().storage::<satay_runtime::storage::BoxedStorage>()`.
For compact strings, use `satay_runtime::StringPolicy<CompactString>`.

Owned annotations can use `generated::owned::User`, or
`generated::User<'static, BoxedStorage>` for boxed storage. Context constructors
use `Input::try_new_in(&storage, ...)`. Map containers and keys remain concrete;
only map values propagate storage. See [storage families](storage-codec-prototype.md)
for the breaking API migration and custom policy traits.

## Custom request bodies

Custom actions can choose another `RequestBody`. The async reqwest adapter requires
`Into<reqwest::Body> + Send`, its blocking adapter requires
`Into<reqwest::blocking::Body>`, and ureq accepts `AsRef<[u8]>`. These bounds describe
buffer/container interoperability; they do not make response decoding streaming.

## Explicit storage contexts

For an arena policy, build a request using `Api::storage_in(&arena)`, send its
owned bytes through the raw transport API, then call the operation's
`decode_*_response_in(&arena, ResponseParts<&[u8]>)`. The returned model borrows
the arena independently of the buffered response. Existing owned send helpers
keep their Send and owned-decoding contracts. See [storage families](storage-codec-prototype.md)
for the full flow and policy integration requirements.
