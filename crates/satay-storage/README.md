# satay-storage

> Easily turn your data structure libraries to support `no_std` via storage type families.

```rust
# #[cfg(feature = "alloc")] {
use satay_storage::{AllocStorage, BoxedStorage, Map, Storage};

let storage = AllocStorage;
let names = storage.try_contiguous([
    storage.try_text("Milo")?,
    storage.try_text("Luna")?,
])?;

let pets = storage.try_map([(storage.try_text("cats")?, names)])?;
assert_eq!(Map::get(&pets, "cats").unwrap()[0], "Milo");

// Concrete owned results can outlive their context.
let boxed = {
    let storage = BoxedStorage;
    storage.try_contiguous([storage.try_text("Milo")?])?
};

assert_eq!(boxed[0].as_ref(), "Milo");

Ok::<(), core::convert::Infallible>(())
```

| Policy | Text | Contiguous | Map |
| --- | --- | --- | --- |
| `AllocStorage` | `String` | `Vec<T>` | `BTreeMap<String, V>` |
| `BoxedStorage` | `Box<str>` | `Box<[T]>` | `BTreeMap<Box<str>, V>` |
| Downstream arena example | bumpalo `String<'a>` | bumpalo `Vec<'a, T>` | `BTreeMap<BumpString<'a>, V>` |
| Downstream hash example | `String` | `Vec<T>` | local `HashMap<String, V>` wrapper |

`Storage::Error` permits bounded/fallible policies without forcing intermediate
heap buffers. The provided policies use `Infallible` and standard allocation
APIs: allocator failure is not converted into a recoverable error.

`Map` exposes lookup by `&str`, length, and iteration. Keys are exactly the
policy's `Text<'a>`, with no family-wide ordering or hashing bounds. Duplicate
text keys keep the last value. Iteration order is unspecified; the provided tree
policies iterate in lexical order. Cloning, serialization, and mutation bounds
belong at use sites.

The lifetime follows the storage context, independently of input text. Concrete
owned policies erase it. Generic code must retain the context unless additional
bounds establish that the returned representation is owned. Arena policies copy
input text into the arena; borrowing response input and context-aware decoding
are separate concerns for future integration.

External policies need local wrappers under Rust's orphan rules. The complete
[`bumpalo`](https://github.com/zeon256/satay-rs/blob/main/crates/satay-storage/examples/bumpalo.rs)
and [`hash_map`](https://github.com/zeon256/satay-rs/blob/main/crates/satay-storage/examples/hash_map.rs)
examples compile as external consumers:

```text
cargo run -p satay-storage --example bumpalo
cargo run -p satay-storage --example hash_map
```

The bumpalo example uses its `collections` feature. Map nodes still use the global
allocator. Bumpalo is a development dependency only; it is not part of the public
contract. Existing Satay runtime and generated APIs are unchanged.
