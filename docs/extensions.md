# Satay Extensions

Satay accepts OpenAPI vendor extensions under `x-satay` when the spec's shape alone cannot produce the Rust type you want.

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
    
    #[cfg_attr(feature = "serde", serde(with = "satay_runtime::serde_string::as_bool"))]
    pub monitored: bool,
}
```


The wire format stays a string: serde deserializes from a JSON string and serializes back to one. Supported `parse-as` values are `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`, `f32`, `f64`, `bool`, and `offset-datetime`. Float parsing uses `fast-float`; `offset-datetime` generates `satay_runtime::OffsetDateTime`. `bool` also supports integer schemas, accepting `1`, `0`, `"1"`, `"0"`, `true`, and `false`; integer-backed bool fields serialize as `1` or `0`.

## `enum-variants`

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

## `treat-error-as-none`

Use `x-satay.treat-error-as-none` on a struct field to make the generated field type `Option<T>`. When deserialization of the field's value fails, the field resolves to `None` instead of returning an error.

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
