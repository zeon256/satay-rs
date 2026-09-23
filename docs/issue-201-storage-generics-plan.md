# Issue #201: generated data structures generic over Storage

Status: implemented, 2026-09-23. Generated family APIs, context seeds, fallible
constructors, owned aliases and explicit operation decoding are integrated.
See [storage family usage and migration](storage-codec-prototype.md).

Implementation decisions: generated model Clone uses an optional CloneStorage
capability to avoid unbounded recursive projection constraints. Ordinary JSON
Deserialize uses StaticStorage + CollectionStorage; Serde-only builds retain
projected-field bounds. Schema placeholders distinguish policy collections from
transport buffers before the AST parameter pass. The acceptance fixtures cover
these decisions with owned, boxed, compact, arena and read-only policies.

Issue: <https://github.com/zeon256/satay-rs/issues/201>.
Foundation: <https://github.com/zeon256/satay-rs/issues/202> (completed).
Related: #160 (text), #161 (no_std).

## Outcome and scope

Generate schema-derived Rust data structures using one `S: satay_storage::Storage`
policy for text and contiguous collections. Select the policy in consumer code,
without regenerating the client. Keep `AllocStorage` as the default, producing
`String` and `Vec<T>`; support `BoxedStorage` and downstream arena policies.

The generic data structures here are generated models, aliases, unions, operation
inputs and responses. `satay-ir` remains a storage-independent semantic schema
graph; its compiler-side collections do not become generic. Rust representation,
lifetime and trait-bound decisions belong in `satay-codegen-rust`.

Map representation integration, per-field capacities, dedicated heapless support,
transport buffer policies and complete no_std clients are outside this issue.
The existing constrained-string/nutype representation also remains concrete.
Adopting `Storage::Text` is necessary to replace the existing string-only policy
with a single family; it does not promise every string-like scalar is generic.

## Starting implementation (before this change)

- `crates/satay-storage/src/lib.rs` already supplies GATs for `Text`,
  `Contiguous` and `Map`, `Storage::Error`, and fallible context-bound
  `try_text`, `try_contiguous`, and `try_map` construction. Reuse this contract.
- `crates/satay-storage/src/allocated.rs` provides owned allocation and boxed
  policies. The downstream bumpalo example demonstrates the lifetime contract.
- `crates/satay-codegen-rust/src/render/storage.rs` currently applies a
  string-only AST rewrite using `satay_runtime::StringStorage = String`.
  Its array eligibility recurses into the element, so numeric-only arrays do
  not themselves cause storage genericity.
- `render/mod.rs::rust_type` emits concrete `Vec` and `BTreeMap` types.
  `render/endpoint/parts.rs` assumes ordinary collection iteration in query
  encoding. Structs, unions and actions derive traits that need new bounds.
- `satay-runtime` has `Action::decode` and `OwnedAction::decode_owned` as static
  methods. The request consumes the action, and buffered responses retain its
  type rather than an allocator context. Merely adding `S` cannot supply one.
- Projection and lossy field decoding use temporary JSON values and owned
  deserialization bounds. They need explicit attention for arena decoding.
- `crates/satay-codegen/tests/generate/storage.rs` already compiles generated
  consumers with string policies and several feature combinations. Extend this
  coverage, including the existing compact-string behavior.

## Proposed public shape

Illustrative generated types (final naming to be checked in the first slice):

```rust
pub struct Pet<'storage, S: Storage + 'storage = AllocStorage> {
    pub name: S::Text<'storage>,
    pub tags: S::Contiguous<'storage, S::Text<'storage>>,
}

pub struct PetList<'storage, S: Storage + 'storage = AllocStorage> {
    pub pets: S::Contiguous<'storage, Pet<'storage, S>>,
}

pub type Counts<'storage, S = AllocStorage> =
    <S as Storage>::Contiguous<'storage, i64>;
```

Use distinct lifetimes for storage (`'storage`), response input (`'de`) and an
action's API borrow (`'api`). Arena results borrow the supplied storage, not the
temporary response bytes. Do not impose `'de: 'storage` when decoding copies
text into the policy.

Rust does not support default lifetime parameters. A default policy therefore
does not preserve every existing named type annotation. Provide documented owned
aliases, such as `owned::Pet = Pet<'static, AllocStorage>`, and verify that owned
decoding produces values independent of a temporary policy value. Avoid adding
runtime references or phantom fields to models whose projections already use
their generic parameters. Do not tie owned results to a local context merely
to share an implementation with the arena path.

Existing explicit `Pet<CompactString>` / `.string_storage::<CompactString>()`
usage needs migration to a local policy with `Text = CompactString` and
`Contiguous = Vec`. Treat generated API changes as a documented breaking change;
keep the old runtime `StringStorage` trait available for previously generated
clients during migration. A compatibility policy wrapper may ease migration,
but must not become a second independent parameter on new models.

For this issue, keep map containers and their keys concrete (`BTreeMap<String,
V>`), while propagating the family into `V`. Document that boxed/arena policies
do not govern map keys yet. This avoids imposing `Ord` on every policy's text
type. Moving map keys and containers together to `S::Map` is separate work;
do not silently claim full map storage support.

## Implementation sequence

### 1. Prove the public API with a small compiled slice

Before converting the renderer, compile a hand-written consumer covering numeric
arrays, nested `Pet` models and an array alias with allocation, boxed and arena
policies. Prove owned aliases, context lifetimes, policy inference and conditional
trait implementations. Include a policy marker that has no Clone/Debug/Serde
implementations to expose accidental bounds on `S` itself.

Settle the names for policy selection, owned constructors, fallible context
constructors and `decode_*_response_in(&storage, response)`. Preserve default
`Api::new()` and ordinary owned request-builder usage. Pass preconstructed
collections directly; offer iterator convenience only through context-aware
fallible construction. Schema defaults that allocate must propagate policy
errors instead of using the current infallible string conversion.

Add a workspace dependency for `satay-storage` and a documented runtime re-export
for generated clients. Keep the storage crate independent of runtime and Serde.
Model-only generation should be able to reference the storage crate directly
when the no_std generation profile is introduced.

### 2. Compute storage requirements in the Rust backend

Replace string-only eligibility with explicit backend metadata, computed before
rendering. An array always requires the family, including arrays of primitive
numbers, bools, closed enums and constrained scalars. Propagate requirements to a
fixed point through named references, aliases, options, unions, nested arrays,
map values, inline models, inputs and responses.

Carry required lifetime/type parameters and operation-specific bounds in the
backend rendering context. Keep collision-safe generic naming and cover existing
action lifetimes. Account for constrained wrappers and coordinate codecs without
turning scalar implementations into text storage types accidentally.

Prefer rendering projections from `TypeRef` directly over extending the broad
post-render AST rewrite. Schema arrays and transport `Vec<u8>` have different
roles even when their rendered Rust spelling is identical. Migrate the relevant
renderers together and remove the superseded rewrite paths.

### 3. Render family projections and bounds

Update `render/mod.rs`, `render/types/*`, endpoint inputs/responses, API groups,
actions, helper signatures and imports consistently. Render arrays as
`S::Contiguous<'storage, Element>` and eligible strings as `S::Text<'storage>`.

Keep `Storage`'s only global contiguous bound as `AsRef<[T]>`. Generated reads
use a typed slice and `.iter()`, including query encoding and array validation.
Do not require Deref, IntoIterator, indexing, push, Default, From<Vec<_>> or
growability from every family member.

Attach Clone, Debug, PartialEq and codec bounds to the actual projected field
types and implementations that use them. Replace derives where they introduce
unnecessary policy-marker bounds or fail on projections. Avoid expanding
recursive bounds indefinitely; use the compiled recursive-model fixtures as
the check for the bound-generation strategy.

Preserve required/optional/null handling, field renames, enum fallback values,
array order and existing request setters. Raw bytes, unexpected-status payloads,
request URIs, API configuration and transport buffers retain their existing
representations.

### 4. Support owned and context-aware codecs

Keep ordinary Serde decoding available for owned policies where their projected
types implement the necessary traits. Put those bounds on the codec impl, not
the struct or `Storage` trait. Serialize contiguous values through slices so
custom containers need not themselves implement Serialize.

Add generated `DeserializeSeed` implementations carrying `&'storage S`, with
shared runtime helpers behind the existing Serde/JSON features. Decode nested
models and text using the same context throughout. Provide policy-specific
collection seed support through an integration trait outside `satay-storage`,
so downstream local policies can construct directly in an arena without a
required intermediate global `Vec`.

The seed integration needs an explicit prototype: Serde's `SeqAccess` is
fallible, whereas `try_contiguous` accepts an iterator of already decoded
elements. Do not hide element errors by ending iteration early or treat a size
hint as an exact length. An owned Vec seed, boxed seed and downstream bumpalo
seed must demonstrate how element and allocation errors propagate. A policy
may choose staging internally, as BoxedStorage already does; the generic
integration must not mandate it for every policy.

Keep storage construction errors distinguishable from malformed JSON and schema
validation failures. For the explicit context API, prefer an error wrapper
parameterized by `S::Error`; the Serde adapter may need an error side channel to
retain the original error without globally requiring Display or std::error::Error.
Resolve this in the prototype before duplicating seed code across renderers.

Cover tagged and untagged unions, lossy fields, projection, special array codecs
and schema defaults. Where replay of buffered JSON is required, invoke the
context seed against that temporary value and copy storage-backed data into the
context. Results must never borrow the temporary. Preserve current validation
and lossy-field semantics, but do not swallow storage exhaustion as an ordinary
invalid-field fallback. Temporary JSON allocation remains permissible here;
this issue does not promise allocation-free parsing.

### 5. Integrate operation encoding and explicit decode contexts

Keep existing `Action`/`OwnedAction` behavior for policies that support ordinary
owned decoding. Introduce an additive context-aware decode entry point, initially
on generated operation functions, accepting a borrowed storage context explicitly.
Allow request construction for arena-backed inputs without requiring their
response types to implement DeserializeOwned.

Do not make static `Action::decode` discover a context implicitly, or retain an
arena inside a response that borrows it. The initial arena flow is: build request,
send using existing raw transport APIs, then decode the buffered response with
`decode_*_response_in(&storage, response)`. Verify this flow against actual
reqwest and ureq entry points. If a buffered `decode_in` helper is added, give it
a separate context-aware trait rather than weakening existing owned contracts.

Async adapters must retain their existing Send guarantees without requiring all
Storage policies to be Send/Sync. In particular, a bumpalo context must not be
forced through an async action abstraction that requires a shared context to be
Sync. Full arena-aware one-step send ergonomics can follow separately.

### 6. Regression coverage and documentation

Extend the generated-consumer tests, backend tests, runtime codec tests and
storage examples. Update generated fixture manifests for the dependency path,
and migrate compact-string fixtures to a policy implementation. Document policy
selection, owned aliases, context construction, decode errors, map exclusions
and API migration. Update architecture/support docs where the new behavior lands.

## Acceptance tests

| Area | Required evidence |
| --- | --- |
| Propagation | Numeric-only arrays; nested arrays of different element types; aliases, inline/named models, recursive references, options, unions, map values, inputs and responses compile. |
| Owned policies | Default fields remain String/Vec; BoxedStorage fields are Box<str>/Box<[T]>; JSON round trips preserve wire data and values can outlive construction contexts. |
| Arena policy | A downstream wrapper decodes nested generated models with seeds; values survive dropping input bytes but cannot escape the arena (compile-fail test). |
| Minimal contracts | Read-only custom contiguous storage and a policy marker without Clone/Debug/Serde compile for operations that do not need those traits. |
| Failure semantics | A deliberately failing policy exercises text/collection failure, nested element errors, invalid JSON and partial construction without returning a partial result. |
| Existing codecs | Query arrays, optional/defaulted parameters, projection, lossy fields, coordinate/array codecs and tagged/untagged unions retain behavior for applicable policies. |
| Transport boundary | Request bodies and unexpected-status bytes retain their concrete types; owned send flows and explicit arena decode flows compile. |
| Feature gates | Generated consumers compile without Serde, with Serde only, and with JSON; storage still builds core-only and with alloc. |
| Compatibility | Generic-name collisions and existing default request-builder examples pass; explicit string-policy migration is documented and tested. |

Run focused backend/runtime/generated-consumer tests first, then workspace tests
with all features and repository formatting/lint checks. Compile negative
lifetime tests and downstream fixture crates; token snapshots alone are not
sufficient. Use the repository's configured test/lint commands at implementation
time rather than assuming a particular local toolchain wrapper.

For no_std, keep generated model references compatible with core/alloc and
verify the standalone storage crate on a target without std. A model-only
no_std + alloc test is conditional on #161 providing compatible imports and
runtime dependencies. A host build with features disabled is not evidence of
no_std support; do not advertise full no_std models or clients until that target
check actually passes.

## Suggested review boundaries

1. Compiled API/seed prototype and backend storage-requirement metadata.
2. Family-generic rendering, owned codecs and default/boxed consumer migration.
3. Arena seeds, typed construction failures and explicit response decode context.
4. Remaining special-codec coverage, adapter checks and migration documentation.

Treat these as one implementation series: owned-only support is a useful
intermediate checkpoint, but does not complete #201's arena acceptance criteria.
