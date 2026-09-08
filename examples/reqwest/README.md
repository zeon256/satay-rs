# Reqwest Example

This example generates code from `../openapi.yaml` at build time, builds a Sans-IO request with Satay's generated action API, sends it with `reqwest`, and decodes the response with the generated response enum.

The example imports `reqwest` through `satay-reqwest`'s re-export, while keeping a direct `reqwest` dependency in `Cargo.toml` so the application owns reqwest's default feature set.

The OpenAPI document also demonstrates Satay's `x-satay.parse-as` extension for APIs that return typed values as strings and the `x-satay.treat-error-as-none` extension for fields where deserialization errors should produce `None` instead of failing. For example, bus stop codes become integers, coordinates become `f64`, arrival timestamps become `satay_runtime::OffsetDateTime`, and `NextBus`/`NextBus2`/`NextBus3` become `Option<BusArrivalTiming>` because the API may return empty values when no bus is available.

```bash
LTA_ACCOUNT_KEY=your-key cargo run -- 83139 15
```

Arguments are optional. The first argument is `BusStopCode`, and the second is `ServiceNo`.

String schemas with `format: uri` become `satay_runtime::Url`. The `odata.metadata` field in this example is parsed automatically, so `arrival.odata_metadata.host_str()` returns the metadata host. Invalid or relative URLs fail deserialization; absolute URLs such as HTTPS and `mailto:` are supported. Serialization uses the URL crate's normalized representation.

URI-to-URL conversion rejects schemas containing `pattern`, `minLength`, or `maxLength` during code generation because preserving those string constraints is not yet supported.
