# Supported OpenAPI Subset

Satay targets OpenAPI 3.1.x and a small, typed subset.

## Current Support

- YAML or JSON OpenAPI documents.
- `components.schemas` as Rust structs, string enums, primitive aliases, and constrained newtypes.
- Schema types: `string` (`unixtime` recognized specially; other formats are plain strings), `integer` (`int32`, `int64`, `unixtime`, or no format, with Rust integer inference from bounds), `number` (`float`, `double`, or no format), `boolean`, arrays, nullable values expressed as `type: [T, "null"]`, and local `#/components/schemas/...` references. Unformatted non-negative open-ended integers infer `u64`; `unixtime` generates `satay_runtime::OffsetDateTime` from Unix timestamp seconds.
- String enums are generated as strict Rust enums. Open string enum shapes such as `anyOf: [{type: string}, {type: string, enum: [...]}]` generate known variants plus `Other(String)` so unknown values round-trip. `Other` is reserved for that fallback; `Unknown` is allowed as a normal declared enum variant name.
- `anyOf` and `oneOf` unions whose branches are local `#/components/schemas/...` references or inline string enums, rendered as ordered serde-untagged Rust enums. Plain `oneOf` support does not enforce JSON Schema's exact-one validation rule.
- `anyOf` or `oneOf` unions with an OpenAPI `discriminator`, when every branch is a local `#/components/schemas/...` reference to an object struct component and branch structs do not contain the discriminator property. These render as serde internally tagged Rust enums. Unmapped branches use their component schema name as the wire tag value; explicit `mapping` entries may override individual branches and may target local `#/components/schemas/...` refs or bare component schema names.
- Non-Satay vendor extensions on supported union schemas, such as `x-oaiMeta`, are treated as metadata annotations and ignored by generation.
- `allOf` component object schemas whose branches are local component object references or inline object branches, rendered by flattening branch fields into one Rust struct. Component `allOf` must declare at least one branch.
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
- Satay-specific `x-satay.parse-as` hints for string fields whose wire values should become stronger Rust types (`date`, `naive-datetime`, `offset-datetime`, `time`, numeric primitives, and others).
- Satay-specific `x-satay.integer-type` hints for overriding inferred Rust integer primitives.
- Satay-specific `x-satay.enum-variants` hints for naming generated Rust enum variants.
- Satay-specific `x-satay.treat-error-as-none` hints for struct fields where deserialization errors should produce `None` instead of failing.
- Validation constraints rendered through `nutype` for:
  - string `minLength` / `maxLength`
  - string `pattern` through `nutype`'s `regex` validator
  - integer and number `minimum` / `maximum` with `exclusiveMinimum` / `exclusiveMaximum`
  - array `minItems` / `maxItems`

## Not Supported Yet

These are known gaps rather than silent compatibility promises:

- OpenAPI 3.0.x documents.
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
- `anyOf`/`oneOf` inline object branches, inline non-enum primitive branches outside the supported open string enum shape, `anyOf`/`oneOf` parameters, recursive union cycles, full JSON Schema `anyOf`/`oneOf` validation semantics, discriminator union branches that are ref-only component aliases, multiple discriminator mapping values targeting the same branch, remote or absolute discriminator mapping targets, OpenAPI `allOf` base discriminator/inheritance patterns, `allOf` outside `components.schemas`, empty `allOf` arrays, and `allOf` scalar/intersection semantics. Empty `allOf: []` is rejected with `EmptyAnyOf` (the same error as empty `anyOf`) because both share the empty composition-shape validation path.
- JSON Schema boolean schemas (`true` / `false`).
- `$ref` siblings other than Satay-owned `x-satay` extensions.
- Non-string enums.
- Numeric `multipleOf`.
- Array `uniqueItems`.
- Object `minProperties` / `maxProperties`.

## Roadmap

- Schema coverage: maps, inline objects, and broader composition semantics.
- Parameter support: header/cookie parameters and OpenAPI style/explode encoding.
- Body support beyond JSON: form data, multipart, and bytes.
- Validated schema references and remote reference loading.
- First-class examples for common transports, keeping generated clients Sans-IO.
