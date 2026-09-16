# Semantic IR migration parity

Issue #212 is the fifth change in the #125 migration stack. Production
`generate` and `generate_with` still use the legacy pipeline. The semantic
entry and its adapter are private and compiled only for tests.

## Parity gate

The generation integration suite still calls the public facade. Its test
modules are also compiled into `parse::parity`, where a local adapter generates
through `normalize_for_rust` and Rust lowering. For every concrete input,
including specifications assembled at runtime, the adapter compares both root
options against the facade and returns the semantic result to the original
assertions.

Successful results must have identical ordered relative paths and file contents.
Rejected results must match their public error variants, contextual fields,
nested payloads, parser positions, and displayed messages. The fixture-file
corpus includes rejections; source-literal discovery remains supplemental and
excludes non-parsing format templates. Dedicated cases cover parsing failures
and competing errors across stages and encounter orders.

The shared tests compile and execute generated clients, including request
encoding, response decoding, constraints/defaults, codecs/coordinates,
projections, ignored fields, naming, unions, response precedence, and existing
storage generics. Both `mod.rs` and `lib.rs` layouts are compiled with default,
no-default, and serde-only features, with JSON behavior exercised under the
default features. Temporary crates share `target/generated-tests` to reuse
dependency builds.

Semantic assertions remain independent of rendered output. They check facts
Rust can collapse or ignore, including requiredness versus nullability,
explicit null defaults, bounds, formats, local decoding policies, shared
identity, ignored wire schemas, composition order, projections, and provenance.
A migration-route retention test drops the source before inspecting its IR.

## Compatibility changes

- Deferred diagnostics carry `DiagnosticKind` and typed owned payloads.
  Codegen translates these back into the existing public error variants;
  lowering no longer dispatches on debug-derived codes or rewrites messages.
  Extension errors originate from JSON values and preserve their data-error
  category, zero text coordinates, message, and extension path.
- Coordinate target checks occur in Rust lowering on the migration route,
  preserving terminal-alias names, target-shape diagnostics, and numeric-field
  rejection behavior. Strict normalization retains its semantic checks.
- Response projections retain `unwrap_required` and `map_required` separately
  from declared nullability. Rust lowering uses them to preserve optional
  outputs/items, even if unselected envelope constraints cannot be normalized.
- Unknown API-key locations remain available as
  `ApiKeyLocation::Unsupported(String)`. Rust ignores those schemes as the
  legacy generator does; strict normalization still rejects them.

There is no production migration switch, legacy fallback in the semantic
adapter, new storage-family integration, or source-document input to Rust
lowering. The preceding four jj changes are unchanged. Production cutover and
removal of temporary differential plumbing belong to #213.

## Focused commands

```sh
cargo test -p satay-codegen --lib parse::parity --offline -- --test-threads=2
cargo test -p satay-codegen --lib parse::rust::tests --offline
cargo test -p satay-codegen --lib parse::tests::ir --offline
cargo test -p satay-codegen --test generate optional_projected_fields --offline
cargo test -p satay-ir --test http --offline
```

Generated-crate dependency preparation uses the existing fetch helper; Cargo's
outer `--offline` does not disable that helper. Clippy and Dylint follow the affected-package commands in
[the quality workflow](../.github/workflows/qc.yml).
