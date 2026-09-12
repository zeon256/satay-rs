#![no_std]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
#![cfg_attr(
    feature = "alloc",
    doc = concat!(
        "\n## Arena example\n\n```rust\n",
        include_str!("../examples/bumpalo.rs"),
        "\n```\n\nArena-backed values cannot escape their context:\n\n",
        "```compile_fail,E0597\n",
        include_str!("../examples/bumpalo.rs"),
        "\nfn escape_arena() {\n",
        "    let text;\n",
        "    {\n",
        "        let storage = ArenaStorage::default();\n",
        "        text = storage.try_text(\"Milo\").unwrap();\n",
        "    }\n",
        "    assert_eq!(text.as_str(), \"Milo\");\n",
        "}\n```\n"
    )
)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
mod allocated;

#[cfg(feature = "alloc")]
pub use allocated::{AllocStorage, BoxedStorage};

/// Read-only access to a map whose keys represent UTF-8 text.
///
/// Each distinct string has at most one entry. Key identity and lookup use the
/// text returned by `AsRef<str>`. Iteration order is unspecified; consumers that
/// need deterministic serialization must choose an appropriate policy.
/// Implementations need not expose mutation, ordering, or hashing.
pub trait Map {
    /// The stored text representation.
    type Key: AsRef<str>;
    /// The stored value.
    type Value;

    /// Returns each key/value pair exactly once, in unspecified order.
    fn iter(&self) -> impl Iterator<Item = (&Self::Key, &Self::Value)>;

    /// Returns the number of distinct keys.
    fn len(&self) -> usize;

    /// Returns whether the map contains no entries.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Looks up an entry by text. The default implementation scans the entries.
    fn get(&self, key: &str) -> Option<&Self::Value> {
        self.iter()
            .find(|(stored, _)| stored.as_ref() == key)
            .map(|(_, value)| value)
    }
}

/// Selects representations and constructs values using a storage context.
///
/// The lifetime belongs to the context, not the input string or iterator. Owned
/// policies can erase it; arenas can retain it. Values within a collection may
/// themselves borrow data. No cloning, serialization, growth, ordering, or hash
/// requirements are imposed on the family.
///
/// Constructors consume their iterators. On error, no partial result is returned;
/// consumed items are not returned to the caller. A context (such as an arena)
/// need not reclaim allocations or roll back state on failure. Implementations
/// document their own resource limits and allocation failure behavior.
pub trait Storage {
    /// A construction error; infallible policies use `core::convert::Infallible`.
    type Error;

    /// UTF-8 text, independent of the lifetime of the input passed to `try_text`.
    type Text<'a>: AsRef<str>
    where
        Self: 'a;

    /// A collection exposing all its elements as one contiguous slice.
    type Contiguous<'a, T: 'a>: AsRef<[T]>
    where
        Self: 'a;

    /// A map using exactly this policy's text representation as its key type.
    type Map<'a, V: 'a>: Map<Key = Self::Text<'a>, Value = V>
    where
        Self: 'a;

    /// Copies text into storage associated with this context.
    ///
    /// # Errors
    /// Returns the policy's construction error if the text cannot be stored.
    fn try_text<'a>(&'a self, value: &str) -> Result<Self::Text<'a>, Self::Error>;

    /// Collects elements in input order without requiring an intermediate `Vec`.
    ///
    /// # Errors
    /// Returns the policy's construction error if the elements cannot be stored.
    fn try_contiguous<'a, T: 'a>(
        &'a self,
        values: impl IntoIterator<Item = T>,
    ) -> Result<Self::Contiguous<'a, T>, Self::Error>;

    /// Collects entries with already constructed text keys.
    ///
    /// For duplicate key text, the last value wins. Which equivalent key object
    /// is retained is unspecified. Key construction errors can be handled before
    /// calling this method. The result's iteration order is unspecified.
    ///
    /// # Errors
    /// Returns the policy's construction error if the entries cannot be stored.
    fn try_map<'a, V: 'a>(
        &'a self,
        entries: impl IntoIterator<Item = (Self::Text<'a>, V)>,
    ) -> Result<Self::Map<'a, V>, Self::Error>;
}
