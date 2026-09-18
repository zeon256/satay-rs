# Semantic IR migration parity

Issue #212 established the parity gate for the fifth change in the #125
migration stack. Issue #213 completed the cutover: production `generate` and
`generate_with` now normalize into `satay-ir` and lower that owned semantic
graph into the Rust model before rendering.

## Parity gate

The generation integration suite calls the public facade, so compilation,
request encoding, response decoding, diagnostics, and both root layouts now
exercise the production semantic route directly. The former differential
adapter has been removed.

Parity was established before cutover by comparing ordered paths, exact file
contents, public error variants, contextual fields, nested payloads, parser
positions, and displayed messages. Retained regression cases cover parsing
failures and competing errors across stages and encounter orders without
keeping the old route as a test oracle.

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

There is no migration switch, legacy fallback, or source-document input to
Rust lowering. Storage-family integration remains a successor to this
migration.

Rust lowering owns its checked representations, coordinate target proof,
identifier words, and type registry under `parse/rust`. OpenAPI syntax and
selection checks belong to normalization. The obsolete validator and its
frontend intermediate types have been deleted.

## Focused commands

```sh
cargo test -p satay-codegen --lib parse::tests::cutover --offline
cargo test -p satay-codegen --test generate roots --offline
cargo test -p satay-codegen --lib parse::rust::tests --offline
cargo test -p satay-codegen --lib parse::tests::ir --offline
cargo test -p satay-codegen --test generate optional_projected_fields --offline
cargo test -p satay-ir --test http --offline
```

Generated-crate dependency preparation uses the existing fetch helper; Cargo's
outer `--offline` does not disable that helper. Clippy and Dylint follow the
affected-package commands in [the quality workflow](../.github/workflows/qc.yml).
