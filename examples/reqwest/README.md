# Reqwest Example

This example generates code from `../openapi.yaml` at build time, builds a Sans-IO request with Satay's generated action API, sends it with `reqwest`, and decodes the response with the generated response enum.

```bash
LTA_ACCOUNT_KEY=your-key cargo run -- 83139 15
```

Arguments are optional. The first argument is `BusStopCode`, and the second is `ServiceNo`.
