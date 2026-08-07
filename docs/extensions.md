# Satay Extensions

Satay accepts OpenAPI vendor extensions under `x-satay` when the spec's shape alone cannot produce the Rust type you want.

## `skip`

Use `x-satay.skip` on an operation that Satay cannot represent yet, such as a multipart upload or binary download:

```yaml
paths:
  /files:
    post:
      operationId: uploadFile
      x-satay:
        skip: true
      requestBody:
        content:
          multipart/form-data: {}
      responses:
        "204":
          description: Uploaded
```

Skipped operations are not validated or generated. `skip: false` keeps normal behavior. The operation-level `x-satay` value must be an object containing supported operation keys such as `skip` and `output`.

When every operation on a path is skipped, Satay also skips the path-level parameters. Component schemas reachable only from skipped operations are excluded from validation and generation, including transitive references. A schema remains included when a retained operation or an otherwise unreferenced component needs it, preventing dangling generated references.

Skipping does not bypass OpenAPI parsing or the document's reference-resolution pass. The document must still be structurally valid.

## `output`

Use operation-level `x-satay.output` when the JSON response has a wire wrapper but the generated public response should contain a nested payload directly. `unwrap-field` selects one field from the top-level response object:

```yaml
paths:
  /BusServices:
    get:
      operationId: getBusServices
      x-satay:
        output:
          unwrap-field: value
      responses:
        "200":
          description: Bus services
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/BusServicesEnvelope"
```

For a wire response such as:

```json
{
  "odata.metadata": "https://example.test/metadata",
  "value": [{ "ServiceNo": "10" }]
}
```

the generated success response carries the schema type of `value`, such as `Vec<BusService>`, rather than `BusServicesEnvelope`.

Add `map-field` when the unwrapped field is an array of objects and the public payload should contain one field from every item:

```yaml
x-satay:
  output:
    unwrap-field: value
    map-field: Link
```

This projects `{"value": [{"Link": "a"}, {"Link": "b"}]}` into `Vec<String>`. Both selectors must be non-empty and must match declared schema properties. `map-field` requires the unwrapped schema to be an array whose item schema is an object. Optional selected fields become `Option` at the corresponding level. The projection applies to every declared JSON response body for the operation; bodyless responses remain bodyless. An operation with `output` but no JSON response body is rejected.

Generated JSON decoders validate the response's container shape before deserializing the projected payload. Missing selected fields become JSON `null`, so required projected types fail normal serde validation while optional projected types decode as `None`.

## `identifier`

Use `x-satay.identifier` on an object property when its generated public name should differ from its OpenAPI wire name:

```yaml
BusStop:
  type: object
  required: [Description, Latitude, Longitude, RequestIdentifier]
  properties:
    Description:
      type: string
      x-satay:
        identifier: desc
    Latitude:
      type: number
      format: double
      x-satay:
        identifier: lat
    Longitude:
      type: number
      format: double
      x-satay:
        identifier: long
    RequestIdentifier:
      type: string
      x-satay:
        identifier: request-id
```

This generates Rust fields using the requested public semantics while preserving the original wire keys:

```rust
pub struct BusStop {
    #[cfg_attr(feature = "serde", serde(rename = "Description"))]
    pub desc: String,
    #[cfg_attr(feature = "serde", serde(rename = "Latitude"))]
    pub lat: f64,
    #[cfg_attr(feature = "serde", serde(rename = "Longitude"))]
    pub long: f64,
    #[cfg_attr(feature = "serde", serde(rename = "RequestIdentifier"))]
    pub request_id: String,
}
```

The extension value is target-neutral, not a Rust token. It must be one or more lower-kebab-case ASCII words: lowercase letters and digits separated by single hyphens. Satay retains those word boundaries, and the Rust backend renders them as snake_case. Rust keywords use raw identifiers where possible, so `identifier: type` renders as `r#type`. A future backend can apply its own casing and keyword policy to the same words.

`identifier` changes only the generated public symbol. The OpenAPI property key remains the canonical serialization and deserialization name; this extension is therefore distinct from a Serde alias, which accepts additional wire spellings. Omitting `identifier` preserves the existing wire-name-derived behavior exactly. If an explicit identifier collides with another generated Rust field after snake_case normalization and keyword escaping, Satay reports a validation error instead of changing the requested name with a suffix.

`identifier` is valid only on object properties, including beside a property `$ref`. Target-specific names and identifier overrides for schemas, operations, parameters, and enum variants are intentionally outside this extension's scope.

## `ignore`

Use `x-satay.ignore` on an object property that is present on the wire but should not be part of the generated public Rust model:

```yaml
BusArrivalResponse:
  type: object
  additionalProperties: false
  required: [odata.metadata, BusStopCode, Services]
  properties:
    odata.metadata:
      type: string
      format: uri
      x-satay:
        ignore: true
    BusStopCode:
      type: string
    Services:
      type: array
      items:
        type: string
```

This generates a struct without an `odata_metadata` field:

```rust
pub struct BusArrivalResponse {
    pub bus_stop_code: String,
    pub services: Vec<String>,
}
```

With the generated crate's `serde` feature, `odata.metadata` is accepted and discarded during deserialization. Whether the ignored property is required, optional, or nullable does not change the Rust struct, and missing ignored required properties do not cause deserialization to fail.

Serialization is intentionally lossy: an ignored property can never be supplied through or emitted from the generated struct, so deserialize-then-serialize round trips remove it. Do not use `ignore` on a request model when the server requires that property to be sent.

The property's schema and references are still validated. Unlike operation-level `x-satay.skip`, `ignore` does not bypass validation or remove the containing operation or schema; it only prevents the validated property from reaching the generated model. `ignore` is valid only directly on object properties, including beside a property `$ref`.

## Standard `unixtime` Format

Satay supports the OpenAPI format registry's `unixtime` format on `type: integer` and `type: string` schemas. Both generate `satay_runtime::OffsetDateTime` and represent Unix timestamp seconds.

```yaml
StartedAt:
  type: integer
  format: unixtime

StartedAtString:
  type: string
  format: unixtime
```

Integer-backed fields deserialize from JSON numbers and serialize back to numbers. String-backed fields deserialize from JSON strings and serialize back to strings. Path, query, and header parameters encode as decimal seconds.

## `parse-as`

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

ReadingDay:
  type: string
  x-satay:
    parse-as: date

ReadingAt:
  type: string
  x-satay:
    parse-as: naive-datetime

FirstBus:
  type: [string, "null"]
  x-satay:
    parse-as: time

Monitored:
  type: integer
  x-satay:
    parse-as: bool
```

For example, a `Bus` struct with `parse-as` fields generates:

```rust
pub struct Bus {
    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_u32"))]
    pub bus_stop_code: u32,
    
    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_f64"))]
    pub latitude: f64,
    
    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_offset_datetime"))]
    pub estimated_arrival: satay_runtime::OffsetDateTime,

    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_date"))]
    pub reading_day: satay_runtime::Date,

    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_naive_datetime"))]
    pub reading_at: satay_runtime::PrimitiveDateTime,

    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_time::option"))]
    pub first_bus: Option<satay_runtime::Time>,

    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_integer::as_bool"))]
    pub monitored: bool,
}
```


The wire format stays a string: serde deserializes from a JSON string and serializes back to one. Supported string-backed `parse-as` values are `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`, `f32`, `f64`, `bool`, `date`, `naive-datetime`, `offset-datetime`, and `time`. Float parsing uses `fast-float`; `date` generates `satay_runtime::Date` and expects `YYYY-MM-DD` values such as `2024-07-16`; optional query parameters become `Option<satay_runtime::Date>` and encode with `satay_runtime::format_date`. `naive-datetime` generates `satay_runtime::PrimitiveDateTime` and expects `YYYY-MM-DDTHH:mm:ss` values such as `2024-07-16T23:59:00`; optional query parameters encode with `satay_runtime::format_naive_datetime`. `offset-datetime` generates `satay_runtime::OffsetDateTime`; `time` generates `satay_runtime::Time` and expects `HHMM` values such as `0620` or `2352`. Nullable `time` fields generate `Option<satay_runtime::Time>` and treat an empty string as `None`. `bool` also supports integer schemas, accepting `1`, `0`, `"1"`, `"0"`, `true`, and `false`; integer-backed bool fields serialize as `1` or `0`.

## `none-if`

Use `x-satay.none-if` on a struct property with a string-backed `parse-as` when the API uses one or more sentinel strings for an unavailable value:

```yaml
wbgt:
  type: string
  x-satay:
    parse-as: f64
    none-if: [NA, "-"]
```

The list must contain at least one string. Matching is exact and case-sensitive, with no trimming or normalization. Empty strings are valid configured values. Satay checks the raw string against the list and then invokes the normal `parse-as` parser for every non-match, so an unexpected invalid value still fails deserialization.

A field with `none-if` generates `Option<T>`. Required, non-null fields still reject missing keys and JSON `null`; optional fields accept missing keys and `null` as `None`; required nullable fields accept `null`. Existing parser-specific behavior is preserved, including numeric and boolean inputs accepted by `parse-as: bool` and blank optional `time` values.

On serialization, `Some(T)` uses the configured string parser. An optional `None` field is omitted. A required `None` field serializes as the first configured sentinel, so multiple accepted sentinel spellings canonicalize to the first list entry.

`none-if` is supported on inline struct properties using the `serde_string` parsers. It is not supported on parameters, `$ref` siblings, integer-backed `bool`, `integer-range`, `number-range`, or union payloads. It cannot be combined with `treat-error-as-none`, because that extension intentionally hides every inner deserialization error while `none-if` preserves errors for unconfigured values.

## `integer-type`

Satay infers the smallest Rust integer primitive for unformatted `type: integer` schemas that declare both `minimum` and `maximum`. Unformatted integer schemas with a one-sided non-negative lower bound and no `maximum` infer `u64`. An explicit integer format (`int32`, `int64`, `uint32`, `uint64`) fixes the primitive instead, and bounds become validation newtypes. Bounds that remain narrower than the primitive still generate validation newtypes.

```yaml
Direction:
  type: integer
  minimum: 1
  maximum: 2
```

This generates a constrained newtype backed by `u8`, because `1..=2` fits in `u8` while still needing validation for the exact allowed range.

Use `x-satay.integer-type` to opt out of inference or pick a specific Rust integer primitive:

```yaml
Direction:
  type: integer
  minimum: 1
  maximum: 2
  x-satay:
    integer-type: i32
```

Supported values are `auto`, `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, and `i64`. `auto` is the default.

## `enum-variants`

Use `x-satay.enum-variants` on string enums when the wire values are terse codes but the Rust variants should be descriptive. Map each wire value to the desired Rust variant name. `Other` is reserved for generated open-enum fallback semantics and cannot be used as an explicit variant name on open enums. Closed enums may use `Other` as a normal explicit variant name.

```yaml
Type:
  type: string
  enum: [SD, DD, BD, ""]
  x-satay:
    enum-variants:
      SD: SingleDecker
      DD: DoubleDecker
      BD: Bendy
      "": Unknown
```

This generates `SingleDecker`, `DoubleDecker`, `Bendy`, and `Unknown` variants with `serde(rename = "...")` attributes where needed. The `Unknown` variant in this example is an ordinary declared variant for the empty string, not a fallback for undeclared wire values.

## `treat-error-as-none`

Use `x-satay.treat-error-as-none` on a struct field to make the generated field type `Option<T>`. When deserialization of the field's value fails, the field resolves to `None` instead of returning an error.

```yaml
BusServiceArrival:
  type: object
  required: [ServiceNo, NextBus]
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

For a referenced field, place `x-satay` directly beside `$ref` as shown above. Satay supports `description`, `x-satay.treat-error-as-none`, `x-satay.ignore`, and `x-satay.identifier` beside a field `$ref`; other `$ref` siblings are rejected instead of being ignored. An `allOf` wrapper is not required or supported for these extensions.
