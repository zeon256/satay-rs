# Upstream provenance

This directory is a source fork of
[`x52dev/oas3-rs`](https://github.com/x52dev/oas3-rs). The complete upstream
source tree is retained; its integration tests, fixtures, and companion crates
are members of Satay's root Cargo workspace and continue to exercise parser
changes.

- Upstream release: `oas3` 0.22.0
- Upstream commit: `42050a631eb0db35574eb13dc53444a746bed4e5`
- Upstream crate path: `crates/oas3`
- License: MIT

The parser package is published as `satay-oas3` while retaining the Rust crate
name `oas3`, allowing Satay's existing imports to remain stable.
