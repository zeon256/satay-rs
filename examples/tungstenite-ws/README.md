# Tungstenite WebSocket Example

This example generates code from `../openapi.yaml`, builds a Satay action, sends
the generated HTTP-shaped request over a local WebSocket connection with
`tokio-tungstenite`, and decodes the WebSocket response through the generated
Satay action API.

The local WebSocket server returns a canned JSON response so the example does not require an LTA API key or external network access.

## How It Is Split

- `src/main.rs`: normal generated-client usage. This is the part an application
  should ideally end up with.
- `src/transport.rs`: the client-side WebSocket transport adapter. It turns a
  Satay action into a request, sends it over WebSocket, receives a response, and
  asks Satay to decode it.
- `src/wire.rs`: the request/response envelope shared by both sides. This is not
  a Satay type; it is the example protocol used to carry HTTP-shaped data over a
  WebSocket message.
- `src/server.rs`: a small local server stub to make the example runnable. In a
  real application this would usually be your own service.
- `src/error.rs`: one error type for the example.

## What Satay Provides

Satay does not know about WebSocket. The generated action only provides the
Sans-IO boundary:

```rust
let request: http::Request<Vec<u8>> = action.request()?;
let decoded = ActionType::decode(satay_runtime::ResponseParts {
    status,
    headers,
    body,
})?;
```

That means the transport only needs to preserve the parts Satay cares about:
method, URI, headers, request body, response status, response headers, and
response body.

## What You Need To Implement

If you already have the WebSocket server, you can ignore `src/server.rs`, but
you still need the client-side transport glue:

- A wire protocol that your server understands. This example uses JSON messages
  with `id`, `method`, `uri`, `headers`, and `body` fields for requests, plus
  `id`, `status`, `headers`, and `body` fields for responses.
- Conversion from `http::Request<Vec<u8>>` into that wire request.
- Conversion from the wire response back into
  `satay_runtime::ResponseParts<Vec<u8>>`.
- A WebSocket client that sends the wire request and waits for the matching
  response. This example uses a simple incrementing `id` so responses can be
  correlated with requests.
- An optional extension trait like `TungsteniteActionExt` so call sites can use
  `.send_over_ws(&mut transport).await` instead of manually calling
  `request()`, `round_trip(...)`, and `decode(...)`.
- Your own operational behavior: timeouts, reconnects, authentication,
  backpressure, concurrent in-flight requests, logging, and close handling.

For one request at a time, the `TungsteniteTransport` in this example is enough
to show the shape. For concurrent requests over one socket, split the WebSocket
sink/stream and keep a pending-response map keyed by the request `id`.

## Run

Run it with:

```sh
cargo run -p satay-example-tungstenite-ws
```
