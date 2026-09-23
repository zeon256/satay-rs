# Generated storage families

Generated schema text and arrays share one `satay_runtime::storage::Storage`
policy. Numeric arrays, nested arrays, aliases, unions, inputs, responses and
recursive models propagate that policy. `satay-ir` remains independent of Rust
storage choices.

## Selecting a policy

`Api::new()` uses `AllocStorage`, whose text and arrays are `String` and `Vec`.
Select another static policy without regenerating the client:

```rust,ignore
use satay_runtime::storage::BoxedStorage;
let api = generated::Api::new().storage::<BoxedStorage>();
```

Generated model signatures include a storage lifetime:

```rust,ignore
pub struct Pet<'storage, S: Storage + 'storage = AllocStorage> {
    pub name: S::Text<'storage>,
    pub tags: S::Contiguous<'storage, S::Text<'storage>>,
}
```

Use `generated::owned::Pet` in owned type annotations. This is an alias for
`Pet<'static, AllocStorage>`. Ordinary JSON deserialization uses a static policy
context and produces values independent of the input buffer or a temporary
construction context. `StaticStorage` supplies that context; `AllocStorage`,
`BoxedStorage` and `StringPolicy<T>` implement it. JSON deserialization also
requires `CollectionStorage`, the policy's Serde integration.

`Input::new(...)` and default input constructors select `AllocStorage` explicitly
to preserve inference. `Input::try_new_in(&storage, ...)` constructs another
family and returns its typed construction error. Required text arguments accept
`AsRef<str>`; collections are passed already constructed. Schema text defaults
are allocated through the same context. Group builders expose `try_operation`
for fallible policies and the usual `operation` for infallible policies.

## Borrowed contexts and transport boundaries

A downstream arena policy implements `Storage` and, for decoding,
`storage_serde::CollectionStorage`. It can use a read-only collection implementing
only `AsRef<[T]>`; no Deref, IntoIterator, growability or Serde implementation is
required on the collection.

```rust,ignore
let api = generated::Api::new().storage_in(&arena);
let model = generated::Pet::<Arena>::from_json_in(&arena, &bytes)?;
drop(bytes); // model borrows the arena, not these bytes
let request = api.untagged().try_save_pet(model)?.request()?;
// Send request through reqwest::Client::execute or ureq::Agent::run.
let result = generated::operations::save_pet::decode_save_pet_response_in(
    &arena,
    satay_runtime::ResponseParts { status, headers, body: &response_bytes },
)?;
```

The response lifetime is tied to `arena`; the response bytes can be dropped.
Generated consumer tests include a rejected attempt to return a model from a
local arena, alongside successful decoding after dropping the input.

Build the request before passing it to an async Send transport future. The raw
request owns its bytes and does not retain the arena. Existing owned
`Action`/`OwnedAction` send helpers retain their contracts; an arena response is
decoded explicitly after buffering the raw response. The reqwest and ureq raw
entry points are checked by compiled downstream fixtures. This does not add a
one-step arena-aware send helper or require the arena to be Send/Sync.

## Codec and value operations

`DecodeContext` carries the storage reference and the first typed policy error.
Text, option, collection, map and model seeds share that context.
`CollectionStorage::deserialize_contiguous` consumes Serde `SeqAccess` directly,
allowing arena allocation without a mandatory global staging Vec. Propagate all
`next_element_seed` errors and treat size hints only as hints. AllocStorage uses
a Vec; BoxedStorage collects and then boxes it.

Explicit decoding returns `DecodeError::Storage(S::Error)` or
`DecodeError::Decode(...)`. Storage errors need neither Display nor Error.
Policies report them through `context.storage_error(error)`. Storage failure
remains fatal through lossy fields and union retries; invalid lossy fields still
become None. Projection and union replay may allocate temporary JSON values, but
the result never borrows them. Parsing is not allocation-free.

Serialization, Debug and equality inspect text and typed slices, including
nested collections, without imposing those traits on the policy marker.
Generated model Clone uses the optional `storage_value::CloneStorage` capability.
Its element callback terminates recursive-model trait resolution and allows
cloning without retaining a runtime context in the model. AllocStorage,
BoxedStorage and StringPolicy implement it. An arena policy need not support
Clone. Action/input/response value traits retain bounds on their fields.

JSON provides the context codecs. Serde-only builds retain ordinary projected
field deserialization, and builds without either feature still support model
construction and request parts. Storage itself remains independent of Serde and
runtime, and builds core-only or with alloc on `thumbv7em-none-eabi`. Generated
clients still use std; full no_std generation remains #161.

## Migration and exclusions

This is a breaking generated API change. Replace `.string_storage::<T>()` with
`.storage::<satay_runtime::StringPolicy<T>>()` to keep an existing compact string
representation and Vec arrays. Replace explicit `Model<T>` with
`Model<'storage, StringPolicy<T>>`, and `Action<'api, T>` with
`Action<'storage, 'api, StringPolicy<T>>`. Existing runtime `StringStorage` remains
available for older generated clients. New clients have one family parameter.

Maps remain `BTreeMap<String, V>`: only their values use the selected family.
Constrained nutype strings, scalar codecs, JSON values, API configuration,
request bytes and unexpected-status bytes retain their concrete representations.
Map storage, per-field capacities and complete no_std clients are separate work.

The renderer marks schema text/collections before applying family projections,
so transport Vecs cannot accidentally become policy arrays. Requirements are
computed to a fixed point before rendering. Compiled fixtures cover default,
boxed, compact and arena policies, numeric/closed-enum/constrained arrays,
recursive models, tagged/untagged unions, query arrays, projection, lossy fields,
fallible defaults and generic-name collisions.
