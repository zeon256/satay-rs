//! Representation-independent serialization selected by schema shape.
use core::fmt;
use core::marker::PhantomData;
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use std::collections::BTreeMap;

/// A schema-selected serialization adapter.
pub trait Codec<T: ?Sized> {
    /// Serializes a value using its schema representation.
    ///
    /// # Errors
    /// Returns the serializer's error.
    fn serialize<W: Serializer>(value: &T, serializer: W) -> Result<W::Ok, W::Error>;
}
/// Uses the value's own Serde implementation.
#[derive(Debug)]
pub struct Native;
impl<T: Serialize + ?Sized> Codec<T> for Native {
    fn serialize<W: Serializer>(value: &T, serializer: W) -> Result<W::Ok, W::Error> {
        value.serialize(serializer)
    }
}
/// Reads text through `AsRef<str>`.
#[derive(Debug)]
pub struct Text;
impl<T: AsRef<str> + ?Sized> Codec<T> for Text {
    fn serialize<W: Serializer>(value: &T, serializer: W) -> Result<W::Ok, W::Error> {
        serializer.serialize_str(value.as_ref())
    }
}
/// Reads contiguous elements through `AsRef<[T]>`.
#[derive(Debug)]
pub struct Array<C, T>(PhantomData<fn() -> (C, T)>);
impl<C: Codec<T>, T, V: AsRef<[T]>> Codec<V> for Array<C, T> {
    fn serialize<W: Serializer>(value: &V, serializer: W) -> Result<W::Ok, W::Error> {
        let values = value.as_ref();
        let mut sequence = serializer.serialize_seq(Some(values.len()))?;
        for value in values {
            sequence.serialize_element(&Encoded::<_, C>::new(value))?;
        }
        sequence.end()
    }
}
/// Serializes optional values using the inner adapter.
#[derive(Debug)]
pub struct Optional<C>(PhantomData<C>);
impl<C: Codec<T>, T> Codec<Option<T>> for Optional<C> {
    fn serialize<W: Serializer>(value: &Option<T>, serializer: W) -> Result<W::Ok, W::Error> {
        match value {
            Some(value) => serializer.serialize_some(&Encoded::<_, C>::new(value)),
            None => serializer.serialize_none(),
        }
    }
}
/// Serializes concrete maps with schema-selected value adapters.
#[derive(Debug)]
pub struct Map<C>(PhantomData<C>);
impl<C: Codec<T>, T> Codec<BTreeMap<String, T>> for Map<C> {
    fn serialize<W: Serializer>(
        value: &BTreeMap<String, T>,
        serializer: W,
    ) -> Result<W::Ok, W::Error> {
        let mut map = serializer.serialize_map(Some(value.len()))?;
        for (key, value) in value {
            map.serialize_entry(key, &Encoded::<_, C>::new(value))?;
        }
        map.end()
    }
}
/// Borrows a value with a schema-selected serialization adapter.
pub struct Encoded<'a, T: ?Sized, C> {
    value: &'a T,
    codec: PhantomData<C>,
}
impl<'a, T: ?Sized, C> Encoded<'a, T, C> {
    /// Creates a serialization view without allocating.
    pub const fn new(value: &'a T) -> Self {
        Self {
            value,
            codec: PhantomData,
        }
    }
}
impl<T: ?Sized, C> fmt::Debug for Encoded<'_, T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Encoded")
    }
}
impl<T: ?Sized, C: Codec<T>> Serialize for Encoded<'_, T, C> {
    fn serialize<W: Serializer>(&self, serializer: W) -> Result<W::Ok, W::Error> {
        C::serialize(self.value, serializer)
    }
}
