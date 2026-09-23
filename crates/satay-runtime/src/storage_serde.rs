//! Serde integration for context-owned text and contiguous collections.
//!
//! The storage lifetime is independent of the deserializer lifetime. A seed
//! copies text into the supplied context; it never retains response bytes.
//! Collection policies consume `SeqAccess` directly, so an arena implementation
//! need not stage elements in a global `Vec`.

#![cfg_attr(feature = "json", doc = concat!(
    "\nArena-backed models cannot escape their context:\n\n```compile_fail,E0597\n",
    include_str!("../tests/support/arena_lifetime.rs"),
    "\nfn main() {\n    let pet;\n    {\n        let arena = Arena(Bump::new());\n        pet = decode_pet(&arena, br#\"\"Milo\"\"#);\n    }\n    assert_eq!(pet.name.as_str(), \"Milo\");\n}\n```\n",
    "\nThe input bytes may be dropped before the model:\n\n```\n",
    include_str!("../tests/support/arena_lifetime.rs"),
    "\nfn main() {\n    let arena = Arena(Bump::new());\n    let pet = {\n        let bytes = br#\"\"Milo\"\"#.to_vec();\n        decode_pet(&arena, &bytes)\n    };\n    assert_eq!(pet.name.as_str(), \"Milo\");\n}\n```\n",
))]

use core::{cell::RefCell, fmt, marker::PhantomData};
#[cfg(feature = "json")]
use serde::de::Error as DeError;
#[cfg(feature = "json")]
use serde_json::de::SliceRead;
use std::borrow;
use std::collections::BTreeMap;

use satay_storage::{AllocStorage, BoxedStorage, Storage};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, DeserializeSeed, SeqAccess, Visitor},
};

/// A decoding failure, preserving the policy's original error without requiring
/// it to implement `Display` or `std::error::Error`.
#[derive(Debug)]
pub enum DecodeError<E, D> {
    /// The storage context could not construct a value.
    Storage(E),
    /// The input was malformed or failed schema validation.
    Decode(D),
}

/// Shared state for one decoding attempt, including replay of temporary values.
///
/// Storage failure is sticky: lossy fields and union retries must never turn it
/// into successful decoding. Call [`Self::finish`] at the outer boundary.
pub struct DecodeContext<'storage, S: Storage + ?Sized> {
    storage: &'storage S,
    error: RefCell<Option<S::Error>>,
}

impl<S: Storage + ?Sized> fmt::Debug for DecodeContext<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecodeContext").finish_non_exhaustive()
    }
}

impl<'storage, S: Storage + ?Sized> DecodeContext<'storage, S> {
    /// Starts a decoding attempt with this context.
    pub const fn new(storage: &'storage S) -> Self {
        Self {
            storage,
            error: RefCell::new(None),
        }
    }

    /// The context that owns decoded text and collections.
    pub const fn storage(&self) -> &'storage S {
        self.storage
    }

    /// Converts a construction failure into a Serde error while retaining its
    /// original value. The first storage error wins.
    pub fn storage_error<D: de::Error>(&self, error: S::Error) -> D {
        let mut slot = self.error.borrow_mut();
        if slot.is_none() {
            *slot = Some(error);
        }
        D::custom("storage construction failed")
    }

    /// Whether a storage error has occurred, including inside a lossy decoder.
    pub fn has_storage_error(&self) -> bool {
        self.error.borrow().is_some()
    }

    /// Completes this attempt, preferring a storage failure over any Serde error
    /// (or apparent success from a decoder that caught the adapter error).
    ///
    /// # Errors
    /// Returns the first storage error or the supplied decoding error.
    pub fn finish<T, D>(self, result: Result<T, D>) -> Result<T, DecodeError<S::Error, D>> {
        match self.error.into_inner() {
            Some(error) => Err(DecodeError::Storage(error)),
            None => result.map_err(DecodeError::Decode),
        }
    }
}

/// Collection decoding implemented by a policy in its Serde integration layer.
///
/// Implementations must propagate every `next_element_seed` error, consume the
/// sequence to its end on success, and treat `size_hint` only as a hint. Report
/// construction failures through `context.storage_error`. No global staging
/// allocation, container mutation API, or Serde impl on the container is required.
pub trait CollectionStorage: Storage {
    /// Decodes elements directly into this policy's contiguous representation.
    ///
    /// # Errors
    /// Returns element, sequence, or storage construction errors.
    fn deserialize_contiguous<'storage, 'de, T: 'storage, A, E>(
        context: &DecodeContext<'storage, Self>,
        sequence: A,
        element: E,
    ) -> Result<Self::Contiguous<'storage, T>, A::Error>
    where
        A: SeqAccess<'de>,
        E: DeserializeSeed<'de, Value = T> + Clone;
}

impl CollectionStorage for AllocStorage {
    fn deserialize_contiguous<'storage, 'de, T: 'storage, A, E>(
        _context: &DecodeContext<'storage, Self>,
        mut sequence: A,
        element: E,
    ) -> Result<Vec<T>, A::Error>
    where
        A: SeqAccess<'de>,
        E: DeserializeSeed<'de, Value = T> + Clone,
    {
        // Do not trust an unbounded size hint from an arbitrary deserializer.
        let mut values = vec![];
        while let Some(value) = sequence.next_element_seed(element.clone())? {
            values.push(value);
        }
        Ok(values)
    }
}

impl CollectionStorage for BoxedStorage {
    fn deserialize_contiguous<'storage, 'de, T: 'storage, A, E>(
        _context: &DecodeContext<'storage, Self>,
        sequence: A,
        element: E,
    ) -> Result<Box<[T]>, A::Error>
    where
        A: SeqAccess<'de>,
        E: DeserializeSeed<'de, Value = T> + Clone,
    {
        AllocStorage::deserialize_contiguous(&DecodeContext::new(&AllocStorage), sequence, element)
            .map(Vec::into_boxed_slice)
    }
}

/// Seed that copies UTF-8 text into a context.
pub struct TextSeed<'context, 'storage, S: Storage + ?Sized>(
    pub &'context DecodeContext<'storage, S>,
);
impl<S: Storage + ?Sized> Copy for TextSeed<'_, '_, S> {}
impl<S: Storage + ?Sized> Clone for TextSeed<'_, '_, S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S: Storage + ?Sized> fmt::Debug for TextSeed<'_, '_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TextSeed")
    }
}
impl<'storage, 'de, S: Storage + ?Sized> DeserializeSeed<'de> for TextSeed<'_, 'storage, S> {
    type Value = S::Text<'storage>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_str(self)
    }
}
impl<'storage, S: Storage + ?Sized> Visitor<'_> for TextSeed<'_, 'storage, S> {
    type Value = S::Text<'storage>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string")
    }
    fn visit_str<D: de::Error>(self, value: &str) -> Result<Self::Value, D> {
        self.0
            .storage
            .try_text(value)
            .map_err(|error| self.0.storage_error(error))
    }
}

/// Seed that decodes a sequence using a policy-provided collection decoder.
pub struct ContiguousSeed<'context, 'storage, S: CollectionStorage, E> {
    /// Shared decoding attempt.
    pub context: &'context DecodeContext<'storage, S>,
    /// Seed reused for each element.
    pub element: E,
}
impl<S: CollectionStorage, E: Clone> Clone for ContiguousSeed<'_, '_, S, E> {
    fn clone(&self) -> Self {
        Self {
            context: self.context,
            element: self.element.clone(),
        }
    }
}
impl<S: CollectionStorage, E> fmt::Debug for ContiguousSeed<'_, '_, S, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ContiguousSeed")
    }
}
impl<'storage, 'de, S: CollectionStorage, E> DeserializeSeed<'de>
    for ContiguousSeed<'_, 'storage, S, E>
where
    E: DeserializeSeed<'de> + Clone,
    E::Value: 'storage,
{
    type Value = S::Contiguous<'storage, E::Value>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'storage, 'de, S: CollectionStorage, E> Visitor<'de> for ContiguousSeed<'_, 'storage, S, E>
where
    E: DeserializeSeed<'de> + Clone,
    E::Value: 'storage,
{
    type Value = S::Contiguous<'storage, E::Value>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an array")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, sequence: A) -> Result<Self::Value, A::Error> {
        S::deserialize_contiguous(self.context, sequence, self.element)
    }
}

/// Ordinary deserialization for scalars that do not depend on storage.
pub struct ValueSeed<T>(PhantomData<fn() -> T>);
impl<T> ValueSeed<T> {
    /// Constructs a scalar seed.
    #[must_use]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}
impl<T> Default for ValueSeed<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> Copy for ValueSeed<T> {}
impl<T> Clone for ValueSeed<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> fmt::Debug for ValueSeed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ValueSeed")
    }
}
impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for ValueSeed<T> {
    type Value = T;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<T, D::Error> {
        T::deserialize(deserializer)
    }
}

/// Decodes one complete JSON document, rejecting trailing input.
///
/// # Errors
/// Returns malformed JSON, validation, or the original storage error.
#[cfg(feature = "json")]
pub fn from_json_slice_in<'storage, S, T, F>(
    storage: &'storage S,
    bytes: &[u8],
    decode: F,
) -> Result<T, DecodeError<S::Error, serde_json::Error>>
where
    S: Storage + ?Sized,
    F: FnOnce(
        &DecodeContext<'storage, S>,
        &mut serde_json::Deserializer<SliceRead<'_>>,
    ) -> Result<T, serde_json::Error>,
{
    let context = DecodeContext::new(storage);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let result = decode(&context, &mut deserializer).and_then(|value| {
        deserializer.end()?;
        Ok(value)
    });
    context.finish(result)
}

/// Seed for an optional value, preserving the element seed's storage context.
#[derive(Clone, Copy, Debug)]
pub struct OptionSeed<E>(pub E);
impl<'de, E: DeserializeSeed<'de>> DeserializeSeed<'de> for OptionSeed<E> {
    type Value = Option<E::Value>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_option(self)
    }
}
impl<'de, E: DeserializeSeed<'de>> Visitor<'de> for OptionSeed<E> {
    type Value = Option<E::Value>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an optional value")
    }
    fn visit_none<D: de::Error>(self) -> Result<Self::Value, D> {
        Ok(None)
    }
    fn visit_unit<D: de::Error>(self) -> Result<Self::Value, D> {
        Ok(None)
    }
    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        self.0.deserialize(deserializer).map(Some)
    }
}

/// Seed for concrete tree maps with owned string keys and context-backed values.
/// Map keys and containers are deliberately outside the generated storage policy.
#[derive(Clone, Copy, Debug)]
pub struct MapSeed<E>(pub E);
impl<'de, E: DeserializeSeed<'de> + Clone> DeserializeSeed<'de> for MapSeed<E> {
    type Value = BTreeMap<String, E::Value>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de, E: DeserializeSeed<'de> + Clone> Visitor<'de> for MapSeed<E> {
    type Value = BTreeMap<String, E::Value>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an object")
    }
    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut values = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            values.insert(key, map.next_value_seed(self.0.clone())?);
        }
        Ok(values)
    }
}

/// Serializes a contiguous value through its slice without requiring a Serde
/// implementation on the container itself.
///
/// # Errors
/// Returns the serializer's error if an element cannot be serialized.
pub fn serialize_contiguous<T: serde::Serialize, C: AsRef<[T]>, W: serde::Serializer>(
    value: &C,
    serializer: W,
) -> Result<W::Ok, W::Error> {
    Serialize::serialize(value.as_ref(), serializer)
}

/// Replays a lossy field using its seed and the same storage context.
///
/// Only malformed field values become `None`. Storage failure aborts decoding
/// and remains available to [`DecodeContext::finish`].
///
/// # Errors
/// Returns invalid input or storage construction errors.
#[cfg(feature = "json")]
pub fn deserialize_lossy<'storage, 'de, S, E, T, D>(
    context: &DecodeContext<'storage, S>,
    seed: E,
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    S: Storage + ?Sized,
    E: for<'value> DeserializeSeed<'value, Value = T>,
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let result = seed.deserialize(value);
    if context.has_storage_error() {
        return Err(DeError::custom("storage construction failed"));
    }
    Ok(result.ok())
}

/// Replays a projected JSON response with the supplied storage context.
///
/// The selected temporary JSON value is owned by the decoder; returned models
/// borrow only `storage`, never that temporary or `bytes`.
///
/// # Errors
/// Returns malformed JSON, invalid projection shapes, schema validation errors,
/// or the original storage construction error.
#[cfg(feature = "json")]
pub fn from_projected_json_slice_in<'storage, S, T, F>(
    storage: &'storage S,
    bytes: &[u8],
    unwrap_field: &str,
    map_field: Option<&str>,
    decode: F,
) -> Result<T, DecodeError<S::Error, crate::Error>>
where
    S: Storage + ?Sized,
    F: FnOnce(&DecodeContext<'storage, S>, serde_json::Value) -> Result<T, serde_json::Error>,
{
    let value =
        crate::project_json_slice(bytes, unwrap_field, map_field).map_err(DecodeError::Decode)?;
    let context = DecodeContext::new(storage);
    let result = decode(&context, value).map_err(crate::Error::from);
    context.finish(result)
}

/// Context decoding implemented by generated storage-generic models.
pub trait DeserializeIn<'storage, S: Storage + 'storage>: Sized {
    /// Copies all storage-backed data into the supplied context.
    ///
    /// # Errors
    /// Returns malformed input, validation, or storage adapter errors.
    fn deserialize_in<'de, D: Deserializer<'de>>(
        context: &DecodeContext<'storage, S>,
        deserializer: D,
    ) -> Result<Self, D::Error>;
}

/// A seed for a generated model, carrying no bounds on the policy marker.
pub struct ModelSeed<'context, 'storage, S: Storage, T> {
    context: &'context DecodeContext<'storage, S>,
    value: PhantomData<fn() -> T>,
}
impl<'context, 'storage, S: Storage, T> ModelSeed<'context, 'storage, S, T> {
    /// Constructs a model seed sharing this decoding attempt.
    pub const fn new(context: &'context DecodeContext<'storage, S>) -> Self {
        Self {
            context,
            value: PhantomData,
        }
    }
}
impl<S: Storage, T> Copy for ModelSeed<'_, '_, S, T> {}
impl<S: Storage, T> Clone for ModelSeed<'_, '_, S, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S: Storage, T> fmt::Debug for ModelSeed<'_, '_, S, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ModelSeed")
    }
}
impl<'storage, 'de, S: Storage, T: DeserializeIn<'storage, S>> DeserializeSeed<'de>
    for ModelSeed<'_, 'storage, S, T>
{
    type Value = T;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<T, D::Error> {
        T::deserialize_in(self.context, deserializer)
    }
}

impl<T: crate::StringStorage + borrow::Borrow<str> + 'static> CollectionStorage
    for crate::StringPolicy<T>
{
    fn deserialize_contiguous<'storage, 'de, V: 'storage, A, E>(
        _context: &DecodeContext<'storage, Self>,
        sequence: A,
        element: E,
    ) -> Result<Vec<V>, A::Error>
    where
        A: SeqAccess<'de>,
        E: DeserializeSeed<'de, Value = V> + Clone,
    {
        AllocStorage::deserialize_contiguous(&DecodeContext::new(&AllocStorage), sequence, element)
    }
}

pub mod serialize;
