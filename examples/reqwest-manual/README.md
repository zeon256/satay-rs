# Reqwest Manual Example

This example generates code from `../openapi.yaml` at build time, builds a request with Satay's generated action API, sends it with `reqwest`, and decodes the response without using `satay-reqwest`.

The important part is the explicit Sans-IO boundary: `action.request()` produces an `http::Request<Vec<u8>>`, the application sends it with its own client, then wraps the returned status, headers, and body in `satay_runtime::ResponseParts` before calling the generated decoder.

```bash
LTA_ACCOUNT_KEY=your-key cargo run -- 83139 15
```

Arguments are optional. The first argument is `BusStopCode`, and the second is `ServiceNo`.
