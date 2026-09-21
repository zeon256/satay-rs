# satay-storage

> Easily turn your data structure libraries to support `no_std` via storage type families.

```rust
#[cfg(feature = "alloc")] {
use satay_storage::{AllocStorage, BoxedStorage, Map, Storage};
use core::convert::Infallible;

fn main() -> Result<(), Infallible> {
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
    
    Ok(())
}
```

| Policy | Text | Contiguous | Map |
| --- | --- | --- | --- |
| `AllocStorage` | `String` | `Vec<T>` | `BTreeMap<String, V>` |
| `BoxedStorage` | `Box<str>` | `Box<[T]>` | `BTreeMap<Box<str>, V>` |
| Downstream arena example | bumpalo `String<'a>` | bumpalo `Vec<'a, T>` | `BTreeMap<BumpString<'a>, V>` |
| Downstream hash example | `String` | `Vec<T>` | local `HashMap<String, V>` wrapper |

External policies need local wrappers under Rust's orphan rules. The complete
[`bumpalo`](https://github.com/zeon256/satay-rs/blob/main/crates/satay-storage/examples/bumpalo.rs)
and [`hash_map`](https://github.com/zeon256/satay-rs/blob/main/crates/satay-storage/examples/hash_map.rs)
examples compile as external consumers:

```text
cargo run -p satay-storage --example bumpalo
cargo run -p satay-storage --example hash_map
```

The bumpalo example uses its `collections` feature. Map nodes still use the global
allocator. 
