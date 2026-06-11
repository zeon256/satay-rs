# Satay Extensions

Satay accepts OpenAPI vendor extensions under `x-satay` when the spec's shape alone cannot produce the Rust type you want.

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

## `integer-type`

Satay infers the smallest Rust integer primitive for `type: integer` schemas that declare both `minimum` and `maximum`. Unformatted integer schemas with a one-sided non-negative lower bound and no `maximum` infer `u64`. Bounds that remain narrower than the primitive still generate validation newtypes.

```yaml
Direction:
  type: integer
  format: int32
  minimum: 1
  maximum: 2
```

This generates a constrained newtype backed by `u8`, because `1..=2` fits in `u8` while still needing validation for the exact allowed range.

Use `x-satay.integer-type` to opt out of inference or pick a specific Rust integer primitive:

```yaml
Direction:
  type: integer
  format: int32
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
