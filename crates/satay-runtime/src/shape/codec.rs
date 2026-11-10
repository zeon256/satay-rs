//! Serde codecs built from the storage read and construction contracts.

use std::fmt;
use std::iter;
use std::marker::PhantomData;

use satay_storage::{Map as Entries, OwnedStorage, Storage};
use serde::de::{self, DeserializeSeed, Deserializer, Error as _, MapAccess, SeqAccess};
use serde::ser::{Serialize, Serializer};

use super::{Contiguous, Map, Model, Optional, Plain, Shape, Text, View, view};

/// Serialization through the storage read contract.
pub trait Encode<'s, S: Storage + 's>: Shape<'s, S> {
    /// Serializes text as a string, collections as sequences, and maps as maps.
    ///
    /// # Errors
    /// Propagates errors from the serializer.
    fn serialize<Z: Serializer>(value: &Self::Value, serializer: Z) -> Result<Z::Ok, Z::Error>;
}

/// Serializes a value of shape `Sh`; usable as a Serde `serialize_with` function.
///
/// # Errors
/// Propagates errors from the serializer.
pub fn serialize<'s, Sh: Encode<'s, S>, S: Storage + 's, Z: Serializer>(
    value: &Sh::Value,
    serializer: Z,
) -> Result<Z::Ok, Z::Error> {
    Sh::serialize(value, serializer)
}

impl<'s, Sh: Encode<'s, S>, S: Storage + 's> Serialize for View<'_, 's, Sh, S> {
    fn serialize<Z: Serializer>(&self, serializer: Z) -> Result<Z::Ok, Z::Error> {
        Sh::serialize(self.0, serializer)
    }
}

impl<'s, S: Storage + 's> Encode<'s, S> for Text {
    fn serialize<Z: Serializer>(value: &Self::Value, serializer: Z) -> Result<Z::Ok, Z::Error> {
        serializer.serialize_str(value.as_ref())
    }
}

impl<'s, S: Storage + 's, E: Encode<'s, S>> Encode<'s, S> for Contiguous<E> {
    fn serialize<Z: Serializer>(value: &Self::Value, serializer: Z) -> Result<Z::Ok, Z::Error> {
        serializer.collect_seq(value.as_ref().iter().map(view::<E, S>))
    }
}

impl<'s, S: Storage + 's, E: Encode<'s, S>> Encode<'s, S> for Map<E> {
    fn serialize<Z: Serializer>(value: &Self::Value, serializer: Z) -> Result<Z::Ok, Z::Error> {
        serializer.collect_map(
            value
                .iter()
                .map(|(key, value)| (key.as_ref(), view::<E, S>(value))),
        )
    }
}

impl<'s, S: Storage + 's, E: Encode<'s, S>> Encode<'s, S> for Optional<E> {
    fn serialize<Z: Serializer>(value: &Self::Value, serializer: Z) -> Result<Z::Ok, Z::Error> {
        match value {
            Some(value) => serializer.serialize_some(&view::<E, S>(value)),
            None => serializer.serialize_none(),
        }
    }
}

/// A generated model decoded into caller-provided storage.
///
/// Decoding copies text into the storage context, so the result never borrows
/// from the input and is bounded only by the storage lifetime `'s`.
pub trait DecodeIn<'de, 's, S: Storage<Error: fmt::Display> + 's>: Sized {
    /// Decodes `Self`, constructing text, collections, and maps in `storage`.
    ///
    /// # Errors
    /// Returns the deserializer's error for malformed input, or a custom error
    /// carrying the policy's construction error.
    fn decode_in<D: Deserializer<'de>>(deserializer: D, storage: &'s S) -> Result<Self, D::Error>;
}

/// Deserialization through the storage construction contract.
pub trait Decode<'de, 's, S: Storage<Error: fmt::Display> + 's>: Shape<'s, S> {
    /// Decodes the described field type, constructing it in `storage`.
    ///
    /// # Errors
    /// Returns the deserializer's error for malformed input, or a custom error
    /// carrying the policy's construction error.
    fn decode<D: Deserializer<'de>>(
        deserializer: D,
        storage: &'s S,
    ) -> Result<Self::Value, D::Error>;
}

/// A [`DeserializeSeed`] that decodes shape `Sh` into a storage context.
pub struct Seed<'s, Sh, S>(&'s S, PhantomData<fn() -> Sh>);

/// Creates the seed decoding shape `Sh` into `storage`.
pub fn seed<Sh, S>(storage: &S) -> Seed<'_, Sh, S> {
    Seed(storage, PhantomData)
}

impl<'de, 's, Sh, S> DeserializeSeed<'de> for Seed<'s, Sh, S>
where
    Sh: Decode<'de, 's, S>,
    S: Storage<Error: fmt::Display> + 's,
{
    type Value = Sh::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        Sh::decode(deserializer, self.0)
    }
}

/// Decodes a string by reference, without materializing an owned `String`.
///
/// # Errors
/// Returns an error if the input is not a string or `build` rejects it.
pub fn text<'de, D, T, X>(
    deserializer: D,
    build: impl FnOnce(&str) -> Result<T, X>,
) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    X: fmt::Display,
{
    struct Visitor<F, T, X>(F, PhantomData<fn() -> (T, X)>);

    impl<F, T, X> de::Visitor<'_> for Visitor<F, T, X>
    where
        F: FnOnce(&str) -> Result<T, X>,
        X: fmt::Display,
    {
        type Value = T;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<T, E> {
            (self.0)(value).map_err(E::custom)
        }
    }

    deserializer.deserialize_str(Visitor(build, PhantomData))
}

impl<'de, 's, S: Storage<Error: fmt::Display> + 's> Decode<'de, 's, S> for Text {
    fn decode<D: Deserializer<'de>>(
        deserializer: D,
        storage: &'s S,
    ) -> Result<Self::Value, D::Error> {
        text(deserializer, |value| storage.try_text(value))
    }
}

struct ShapeVisitor<'s, Sh, S>(&'s S, PhantomData<fn() -> Sh>);

impl<'de, 's, S, E> de::Visitor<'de> for ShapeVisitor<'s, Contiguous<E>, S>
where
    S: Storage<Error: fmt::Display> + 's,
    E: Decode<'de, 's, S>,
{
    type Value = S::Contiguous<'s, E::Value>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a sequence")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let storage = self.0;
        // The policy pulls elements straight from the input; a failed element
        // ends the iteration and is reported in place of the collection.
        let mut failure = None;
        let elements = iter::from_fn(|| {
            seq.next_element_seed(seed::<E, S>(storage))
                .unwrap_or_else(|error| {
                    failure = Some(error);
                    None
                })
        })
        .fuse();
        let collected = storage.try_contiguous(elements);
        match failure {
            Some(error) => Err(error),
            None => collected.map_err(A::Error::custom),
        }
    }
}

impl<'de, 's, S, E> de::Visitor<'de> for ShapeVisitor<'s, Map<E>, S>
where
    S: Storage<Error: fmt::Display> + 's,
    E: Decode<'de, 's, S>,
{
    type Value = S::Map<'s, E::Value>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a map")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let storage = self.0;
        let mut failure = None;
        let entries = iter::from_fn(|| {
            map.next_entry_seed(seed::<Text, S>(storage), seed::<E, S>(storage))
                .unwrap_or_else(|error| {
                    failure = Some(error);
                    None
                })
        })
        .fuse();
        let collected = storage.try_map(entries);
        match failure {
            Some(error) => Err(error),
            None => collected.map_err(A::Error::custom),
        }
    }
}

impl<'de, 's, S, E> de::Visitor<'de> for ShapeVisitor<'s, Optional<E>, S>
where
    S: Storage<Error: fmt::Display> + 's,
    E: Decode<'de, 's, S>,
{
    type Value = Option<E::Value>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an optional value")
    }

    fn visit_none<X: de::Error>(self) -> Result<Self::Value, X> {
        Ok(None)
    }

    fn visit_unit<X: de::Error>(self) -> Result<Self::Value, X> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        E::decode(deserializer, self.0).map(Some)
    }
}

impl<'de, 's, S, E> Decode<'de, 's, S> for Contiguous<E>
where
    S: Storage<Error: fmt::Display> + 's,
    E: Decode<'de, 's, S>,
{
    fn decode<D: Deserializer<'de>>(
        deserializer: D,
        storage: &'s S,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(ShapeVisitor::<Self, S>(storage, PhantomData))
    }
}

impl<'de, 's, S, E> Decode<'de, 's, S> for Map<E>
where
    S: Storage<Error: fmt::Display> + 's,
    E: Decode<'de, 's, S>,
{
    fn decode<D: Deserializer<'de>>(
        deserializer: D,
        storage: &'s S,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(ShapeVisitor::<Self, S>(storage, PhantomData))
    }
}

impl<'de, 's, S, E> Decode<'de, 's, S> for Optional<E>
where
    S: Storage<Error: fmt::Display> + 's,
    E: Decode<'de, 's, S>,
{
    fn decode<D: Deserializer<'de>>(
        deserializer: D,
        storage: &'s S,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_option(ShapeVisitor::<Self, S>(storage, PhantomData))
    }
}

impl<'de, 's, S, T> Decode<'de, 's, S> for Plain<T>
where
    S: Storage<Error: fmt::Display> + 's,
    T: de::Deserialize<'de> + 's,
{
    fn decode<D: Deserializer<'de>>(deserializer: D, _: &'s S) -> Result<T, D::Error> {
        T::deserialize(deserializer)
    }
}

impl<'de, 's, S, T> Decode<'de, 's, S> for Model<T>
where
    S: Storage<Error: fmt::Display> + 's,
    T: DecodeIn<'de, 's, S> + 's,
{
    fn decode<D: Deserializer<'de>>(deserializer: D, storage: &'s S) -> Result<T, D::Error> {
        T::decode_in(deserializer, storage)
    }
}

macro_rules! leaf {
    ($($marker:ident),*) => {$(
        impl<'s, S: Storage + 's, T: Serialize + 's> Encode<'s, S> for $marker<T> {
            fn serialize<Z: Serializer>(value: &T, serializer: Z) -> Result<Z::Ok, Z::Error> {
                value.serialize(serializer)
            }
        }
    )*};
}

leaf!(Plain, Model);

/// Decodes a model through its policy's context-free instance, for plain
/// `Deserialize` implementations.
///
/// # Errors
/// Returns the deserializer's error for malformed input, or a custom error
/// carrying the policy's construction error.
pub fn deserialize<'de, 's, T, S, D>(deserializer: D) -> Result<T, D::Error>
where
    T: DecodeIn<'de, 's, S>,
    S: OwnedStorage<Error: fmt::Display>,
    D: Deserializer<'de>,
{
    T::decode_in(deserializer, S::CONTEXT)
}

/// Checks that an object key has not already filled `slot`.
///
/// # Errors
/// Returns a duplicate-field error naming `field` when the slot is filled.
pub fn vacant<E: de::Error, T>(slot: &Option<T>, field: &'static str) -> Result<(), E> {
    match slot {
        Some(_) => Err(E::duplicate_field(field)),
        None => Ok(()),
    }
}

/// Takes a field that every valid object must contain.
///
/// # Errors
/// Returns a missing-field error naming `field` when the slot is empty.
pub fn required<E: de::Error, T>(slot: Option<T>, field: &'static str) -> Result<T, E> {
    slot.ok_or_else(|| E::missing_field(field))
}

/// Identifies an object key by its position in a list of wire names.
///
/// Unknown keys yield the list length, so callers can skip their values.
#[derive(Debug, Clone, Copy)]
pub struct Field(pub &'static [&'static str]);

impl<'de> DeserializeSeed<'de> for Field {
    type Value = usize;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<usize, D::Error> {
        deserializer.deserialize_identifier(self)
    }
}

impl de::Visitor<'_> for Field {
    type Value = usize;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a field name")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<usize, E> {
        self.visit_bytes(value.as_bytes())
    }

    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<usize, E> {
        Ok(self
            .0
            .iter()
            .position(|field| field.as_bytes() == value)
            .unwrap_or(self.0.len()))
    }
}
