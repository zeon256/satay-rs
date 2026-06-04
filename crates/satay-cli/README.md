# satay-cli

Command-line interface for Satay.

Install from this repository:

```bash
cargo install --path crates/satay-cli
```

Generate Rust client code from an OpenAPI document:

```bash
satay generate --input openapi.yaml --output src/generated --rustfmt
```

Use `--lib` to emit `lib.rs` instead of `mod.rs` at the output root.

If you are looking for `satay-rs`, visit the [main repository](https://github.com/zeon256/satay-rs).
