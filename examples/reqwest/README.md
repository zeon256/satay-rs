# Reqwest Example

This example generates code from `../openapi.yaml` at build time, builds a Sans-IO request with Satay's generated action API, sends it with `reqwest`, and decodes the response with the generated response enum.

The example imports `reqwest` through `satay-reqwest`'s re-export, while keeping a direct `reqwest` dependency in `Cargo.toml` to choose the TLS feature.

The OpenAPI document also demonstrates Satay's `x-satay.parse-as` extension for APIs that return typed values as strings and the `x-satay.treat-error-as-none` extension for fields where deserialization errors should produce `None` instead of failing. For example, bus stop codes become integers, coordinates become `f64`, arrival timestamps become `satay_runtime::OffsetDateTime`, and `NextBus`/`NextBus2`/`NextBus3` become `Option<BusArrivalTiming>` because the API may return empty values when no bus is available.

```bash
LTA_ACCOUNT_KEY=your-key cargo run -- 83139 15
```

Arguments are optional. The first argument is `BusStopCode`, and the second is `ServiceNo`.
