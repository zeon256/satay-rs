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

## Storage policies

Generated models, inputs, and responses that contain text, arrays, or maps take a
storage policy and the lifetime of its context: `BusService<'s, S>`. The policy is a
[`satay_storage::Storage`](../crates/satay-storage) type family, re-exported from
`satay_runtime`, and each field uses its member directly:

| OpenAPI | Field type | `AllocStorage` (default) | `BoxedStorage` |
| --- | --- | --- | --- |
| `string` | `S::Text<'s>` | `String` | `Box<str>` |
| `array` | `S::Contiguous<'s, T>` | `Vec<T>` | `Box<[T]>` |
| map | `S::Map<'s, V>` | `BTreeMap<String, V>` | `BTreeMap<Box<str>, V>` |

One policy propagates through nested models, aliases, unions, open-enum unknown
values, operation inputs, and responses, including arrays that hold no text.
Constrained newtypes keep `String`/`Vec`; parsed values, closed enums, `JsonValue`,
API configuration, and request/response byte buffers keep their representations.

Select an owned policy for a client without regenerating it:

```rust
let api = generated::Api::new().storage::<satay_runtime::BoxedStorage>();
let decoded = api.bus().get_services().send_with(&client).await?;
```

The client's policy covers request inputs and responses decoded without a storage
context (`send_with`, `decode`, `decode_owned`). Those need
`satay_runtime::OwnedStorage`: a policy whose values never borrow from a context.

### Decoding into caller-provided storage

A buffered response decodes into any policy, chosen at the call. Text is copied into
the storage context, so the result borrows from that context and not from the
response buffer:

```rust
let arena = ArenaStorage::default(); // your `Storage` over, say, a bumpalo arena
let response = api.bus().get_services().send_buffered_with(&client).await?;
let services = response.decode_in(&arena)?; // GetBusServicesResponse<'_, ArenaStorage>
```

`satay_runtime::from_json_slice_in(body, &storage)` decodes a single model the same
way, and every generic response has a `decode_<operation>_response_in` helper.
Decoding into a context is fallible through the policy's `Storage::Error`, which is
reported as a decode error. A context need not reclaim what a rejected value (a
failed union branch, a lossy field) already allocated.

### Writing a policy

Implement `Storage` for a local type: three associated types and their
constructors. Generated models derive everything else from that contract, so the
policy and its containers need no Serde, `Debug`, `Clone`, or comparison support.
Add `OwnedStorage` when values own their storage:

```rust
struct Compact;

impl satay_runtime::Storage for Compact {
    type Error = std::convert::Infallible;
    type Text<'a> = compact_str::CompactString;
    type Contiguous<'a, T: 'a> = Vec<T>;
    type Map<'a, V: 'a> = std::collections::BTreeMap<compact_str::CompactString, V>;
    // try_text, try_contiguous, try_map
}

impl satay_runtime::OwnedStorage for Compact {
    const CONTEXT: &'static Self = &Self;
}
```

| Capability | Requires |
| --- | --- |
| `Debug`, `PartialEq`, `Serialize`, request encoding, `decode_in` | `S: Storage` (`Storage::Error: Display` to decode) |
| `Deserialize`, `send_with`, `decode` | `S: OwnedStorage` |
| `Clone`, text parameter defaults | `S: OwnedStorage<Error = Infallible>` |

Transport adapters send values implementing `satay_runtime::Action`. An action
whose response is storage-generic implements it for owned client policies only;
with any other client policy, call `request()` and `decode_in` directly.

Map equality ignores entry order, and map iteration order is the policy's.

### Naming the policy

Field types are projections of the policy, so a value alone does not identify it.
Type positions use the default; expressions without type context need it named:

```rust
let user: generated::User = generated::User { name: "Ada".to_owned(), tags: vec![] };
let input = generated::GetUserInput::<satay_runtime::BoxedStorage>::new("42");
let user: generated::User<'_, satay_runtime::BoxedStorage> = serde_json::from_slice(body)?;
```

Values passed to client methods take the client's policy and need no annotation.

Decoding always copies text into storage; borrowing strings from the response
buffer is not a storage policy. The runtime GAT and `from_json_slice` still support
custom actions that borrow, and such models cannot outlive their
`BufferedResponse`. Projected generated responses deserialize through an owned
JSON value before entering storage.

## Custom request bodies

Custom actions can choose another `RequestBody`. The async reqwest adapter requires
`Into<reqwest::Body> + Send`, its blocking adapter requires
`Into<reqwest::blocking::Body>`, and ureq accepts `AsRef<[u8]>`. These bounds describe
buffer/container interoperability; they do not make response decoding streaming.
