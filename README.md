# Satay

<p align="center">
  <img src="logo.png" alt="satay-rs logo" width="300">
  <br>
  <sub><i>Disclaimer: I can't design. This logo was generated using ChatGPT.</i></sub>
</p>

Satay is a Rust OpenAPI client generator that generates sans-IO client code with strong validation through newtypes. It focuses on generating ergonomic Rust code that captures OpenAPI constraints in the type system, while leaving transport choices to the user. The aim of generating sans-IO code is to maximize flexibility: you can use the generated clients in any Rust environment, with any HTTP client, and even in non-network contexts like tests or WASM. 

> [!WARNING]
> This is work in progress and it is designed to support the generation of lta-rs clients first. Features and support for other OpenAPI specs will be added as needed as the priority is to get a working lta-rs client with good ergonomics and validation. Contributions are welcome!

```bash
satay generate --input openapi.yaml --output src/generated --rustfmt
```

If `--output` ends in `.rs`, Satay writes that file. Otherwise it writes `mod.rs` inside the output directory.

## Current support

Satay targets OpenAPI 3.0.x and a small, typed subset.

- YAML or JSON OpenAPI documents.
- `components.schemas` as Rust structs, string enums, primitive aliases, and constrained newtypes.
- Schema types: `string`, `integer` (`int32`, `int64`, or no format), `number` (`float`, `double`, or no format), `boolean`, arrays, nullable values, and local `#/components/schemas/...` references.
- Operations for standard HTTP methods with explicit `operationId`, or inferred names from method + path.
- Path, query, and header parameters declared with `schema`.
- Path-level parameters with operation-level overrides.
- JSON request bodies using `application/json` or structured JSON media types such as `application/problem+json`.
- Generated `<operation>_parts` functions that return `satay_runtime::RequestParts<B>` without requiring an HTTP client.
- Generated input constructors and chainable setters for optional operation inputs.
- Generated `SERVER_URL` from the first OpenAPI `servers` entry.
- Generated `encode_<operation>` helpers, behind the generated crate's `json` feature, that produce `http::Request<Vec<u8>>`.
- Generated `decode_<operation>_response` helpers, behind the generated crate's `json` feature, that decode known JSON responses and preserve unknown statuses as `UnexpectedStatus(http::StatusCode, Vec<u8>)`.
- Generated `Api` action builders, behind the generated crate's `json` feature, for base URL, API-key auth, request construction, and response decoding without choosing a transport.
- Header and query `apiKey` security schemes for generated action builders.
- Percent-encoded path and query values.
- Repeated query parameters for array query values.
- Optional fields and optional request bodies.
- `serde` derives and field renames behind the generated crate's `serde` feature.
- Satay-specific `x-satay.parse-as` hints for string fields whose wire values should become stronger Rust types.
- Satay-specific `x-satay.enum-variants` hints for naming generated Rust enum variants.
- Satay-specific `x-satay.treat-error-as-none` hints for struct fields where deserialization errors should produce `None` instead of failing.
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

## Generated `nutype` Examples

Satay creates named `nutype` wrappers for constrained component schemas, inline constrained object fields, and inline constrained operation parameters. For example, constrained component schemas such as:

```yaml
Age:
  type: integer
  format: int32
  minimum: 0
  maximum: 130
Email:
  type: string
  pattern: '^[^@]+@[^@]+\.[^@]+$'
User:
  type: object
  required:
    - age
    - name
    - score
  properties:
    age:
      $ref: '#/components/schemas/Age'
    email:
      $ref: '#/components/schemas/Email'
    name:
      type: string
      minLength: 1
      maxLength: 80
    score:
      type: number
      format: float
      minimum: 0
      maximum: 1
```

generate Rust like:

```rust
#[nutype::nutype(
    validate(greater_or_equal = 0, less_or_equal = 130),
    derive(
        Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct Age(i32);

#[nutype::nutype(
    validate(regex = "^[^@]+@[^@]+\\.[^@]+$"),
    derive(
        Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct Email(String);

#[nutype::nutype(
    validate(len_char_min = 1, len_char_max = 80),
    derive(
        Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct UserName(String);

#[nutype::nutype(
    validate(finite, greater_or_equal = 0.0, less_or_equal = 1.0),
    derive(Debug, Clone, Copy, PartialEq, PartialOrd, AsRef, Deref, TryFrom, Into, Display),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct UserScore(f32);

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct User {
    pub age: Age,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub email: Option<Email>,
    pub name: UserName,
    pub score: UserScore,
}
```

Inline constrained operation parameters get operation-scoped names:

```rust
#[nutype::nutype(
    validate(regex = "^[a-zA-Z0-9-]+$", len_char_min = 3, len_char_max = 20),
    derive(
        Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct GetUserUserIdParameter(String);

#[nutype::nutype(
    validate(greater_or_equal = 1, less_or_equal = 100),
    derive(
        Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct GetUserLimitParameter(i32);

#[nutype::nutype(
    validate(len_char_min = 2),
    derive(
        Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display
    ),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct GetUserTagsParameterItem(String);

#[nutype::nutype(
    validate(predicate = |items| items.len() >= 1 && items.len() <= 3),
    derive(Debug, Clone, PartialEq, AsRef, Deref, TryFrom, Into),
    cfg_attr(feature = "serde", derive(Serialize, Deserialize))
)]
pub struct GetUserTagsParameter(Vec<GetUserTagsParameterItem>);
```

Call sites construct constrained values with `try_new` before building requests:

```rust
let user_id = GetUserUserIdParameter::try_new("user-42".to_owned()).expect("valid user id");
let limit = GetUserLimitParameter::try_new(10).expect("valid limit");
let tag = GetUserTagsParameterItem::try_new("rs".to_owned()).expect("valid tag");
let tags = GetUserTagsParameter::try_new(vec![tag]).expect("valid tag list");

let input = GetUserInput::new(user_id).limit(limit).tags(tags);

assert!(Age::try_new(131).is_err());
assert!(Email::try_new("not-an-email".to_owned()).is_err());
```

With the generated crate's `serde` feature enabled, response deserialization uses the same validators, so JSON payloads that violate OpenAPI bounds fail during decoding instead of producing invalid typed values.

## Satay Extensions

Satay accepts OpenAPI vendor extensions under `x-satay` when the spec's shape alone can't produce the Rust type you want.

Use `x-satay.parse-as` on `type: string` schemas when an API sends a value as a JSON string but the Rust field should be a stronger type. Serde deserializes from a string and serializes back to a string, so the wire format stays the same.

```yaml
BusStopCode:
  type: string
  x-satay:
    parse-as: u32

Latitude:
  type: string
  x-satay:
    parse-as: f64

EstimatedArrival:
  type: string
  x-satay:
    parse-as: offset-datetime
```

For example, a `Reading` struct with `parse-as` fields generates:

```rust
pub struct Reading {
    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_u32"))]
    pub id: u32,
    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_f64"))]
    pub value: f64,
    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_u8"))]
    pub count: u8,
    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_offset_datetime"))]
    pub seen_at: satay_runtime::OffsetDateTime,
}
```

The wire format stays a string — serde deserializes from a JSON string and serializes back to one — but the Rust type is `u32`, `f64`, `u8`, or `OffsetDateTime`.

Supported `parse-as` values are `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`, `f32`, `f64`, and `offset-datetime`. Float parsing uses `fast-float`; `offset-datetime` generates `satay_runtime::OffsetDateTime`.

### `enum-variants`

Use `x-satay.enum-variants` on string enums when the wire values are terse codes but the Rust variants should be descriptive. Map each wire value to the desired Rust variant name. Mapping a value to `Unknown` folds it into Satay's generated fallback variant.

```yaml
Type:
  type: string
  enum:
    - SD
    - DD
    - BD
    - ""
  x-satay:
    enum-variants:
      SD: SingleDecker
      DD: DoubleDecker
      BD: Bendy
      "": Unknown
```

This generates `SingleDecker`, `DoubleDecker`, and `Bendy` variants with `serde(rename = "...")` attributes, plus the default `Unknown` fallback.

### `treat-error-as-none`

Use `x-satay.treat-error-as-none` on a struct field to make the generated field type `Option<T>`. When deserialization of the field's value fails (for example, an API returns empty strings where a number is expected), the field resolves to `None` instead of returning an error.

```yaml
BusServiceArrival:
  type: object
  required:
    - ServiceNo
    - NextBus
  properties:
    ServiceNo:
      type: string
    NextBus:
      $ref: "#/components/schemas/BusArrivalTiming"
      x-satay:
        treat-error-as-none: true
```

When `treat-error-as-none` is `true`, the generated Rust field becomes `Option<BusArrivalTiming>` with a custom deserializer that catches any error and returns `None`:

```rust
pub struct BusServiceArrival {
    pub service_no: String,
    #[cfg_attr(feature = "serde", serde(
        rename = "NextBus",
        deserialize_with = "satay_runtime::treat_error_as_none::deserialize",
        serialize_with = "satay_runtime::treat_error_as_none::serialize",
        default,
        skip_serializing_if = "Option::is_none"
    ))]
    pub next_bus: Option<BusArrivalTiming>,
}
```

This is useful for APIs that return empty or malformed values in nested objects when data is unavailable, rather than omitting the field or returning `null`. The `treat-error-as-none` extension requires the generated crate's `json` feature.

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

## Transport Adapters

Satay's core contract is still Sans-IO: generated actions build `http::Request<Vec<u8>>` values and decode `satay_runtime::ResponseParts<B>`. The optional adapter crates only add small extension traits for common clients.

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

For the lower-level path without an adapter crate, see `examples/reqwest-manual`. It calls `action.request()`, sends the request with `reqwest`, builds `satay_runtime::ResponseParts`, and calls the generated decoder directly.

## Not supported yet

These are known gaps rather than silent compatibility promises:

- OpenAPI 3.1.
- Remote or file `$ref`s.
- Retries, non-API-key authentication, server variables, or automatic server selection beyond the first `servers` entry.
- Cookie parameters.
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

- Schema coverage: maps, inline objects, composition, and discriminators.
- Parameter support: header/cookie parameters and OpenAPI style/explode encoding.
- Body support beyond JSON: form data, multipart, and bytes.
- Validated schema references and remote reference loading.
- OpenAPI 3.1 support once the schema subset is explicit and tested.
- First-class examples for common transports, keeping generated clients Sans-IO.

## Examples

- `examples/reqwest`: generates from `examples/openapi.yaml` at build time, sends the request with `reqwest`, and decodes with the generated action API.
- `examples/reqwest-blocking`: uses the same generated action API with `reqwest::blocking`.
- `examples/reqwest-manual`: sends with `reqwest` directly without using `satay-reqwest`.
- `examples/ureq`: sends generated actions with `ureq`.

## Workspace

- `crates/satay-cli`: user-facing `satay` executable.
- `crates/satay-codegen`: OpenAPI parser, normalized IR, and Rust generator.
- `crates/satay-runtime`: small IO-free support crate for generated code.
