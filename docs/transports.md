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
let decoded = generated::GetBusArrivalAction::<String>::decode(response.as_bytes())?;
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

let decoded = generated::GetBusArrivalAction::<String>::decode(response.as_bytes())?;
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

## String storage

Generated models, inputs, and actions use a string type parameter with `String`
as the default. Select another container for an API without regenerating it:

```rust
let api = generated::Api::new().string_storage::<Box<str>>();
// Or CompactString, with compact_str's serde feature enabled.
let decoded = api.bus().get_arrival(83139).send_with(&client).await?;
```

Low-level calls sometimes need explicit types because Rust does not use generic
parameter defaults to resolve every inference ambiguity:

```rust
let input = generated::GetUserInput::<Box<str>>::new("42");
let user: generated::User<Box<str>> = serde_json::from_slice(body)?;
```

`StringStorage` is implemented automatically for containers supporting string
access, conversion from `String`, cloning, debug formatting, equality and ordering.
Generated JSON codecs additionally require Serde serialization and owned
deserialization. Storage propagates through nested models, arrays, maps (including
keys), aliases, and open-enum unknown values. Constrained string newtypes retain
`String`; parsed values, closed enums, `JsonValue`, API configuration, and encoding
buffers keep their existing representations.

Generated borrowed-string decoding is a later step: substituting `Cow` currently
produces owned strings. The runtime GAT and `from_json_slice` already support custom
actions that borrow. Such models cannot outlive their `BufferedResponse`. Projected
generated responses still deserialize through an owned JSON value.

## Custom request bodies

Custom actions can choose another `RequestBody`. The async reqwest adapter requires
`Into<reqwest::Body> + Send`, its blocking adapter requires
`Into<reqwest::blocking::Body>`, and ureq accepts `AsRef<[u8]>`. These bounds describe
buffer/container interoperability; they do not make response decoding streaming.
