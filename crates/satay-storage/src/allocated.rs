use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};
use core::{borrow::Borrow, convert::Infallible};

use crate::{Map, Storage};

/// Global-allocator storage using `String`, `Vec<T>`, and `BTreeMap<String, V>`.
///
/// Construction uses the standard infallible allocation APIs. Allocation failure
/// follows the allocator's behavior rather than returning `Storage::Error`.
/// Values own their allocations and can outlive the storage context.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllocStorage;

impl Storage for AllocStorage {
    type Error = Infallible;
    type Text<'a> = String;
    type Contiguous<'a, T: 'a> = Vec<T>;
    type Map<'a, V: 'a> = BTreeMap<String, V>;

    fn try_text<'a>(&'a self, value: &str) -> Result<Self::Text<'a>, Self::Error> {
        Ok(String::from(value))
    }

    fn try_contiguous<'a, T: 'a>(
        &'a self,
        values: impl IntoIterator<Item = T>,
    ) -> Result<Self::Contiguous<'a, T>, Self::Error> {
        Ok(values.into_iter().collect())
    }

    fn try_map<'a, V: 'a>(
        &'a self,
        entries: impl IntoIterator<Item = (Self::Text<'a>, V)>,
    ) -> Result<Self::Map<'a, V>, Self::Error> {
        Ok(entries.into_iter().collect())
    }
}

/// Global-allocator storage using `Box<str>`, `Box<[T]>`, and tree maps.
///
/// Collections are collected through a `Vec` and converted to boxed slices.
/// Construction uses infallible allocation APIs, as with [`AllocStorage`].
/// Values own their allocations and can outlive the storage context.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoxedStorage;

impl Storage for BoxedStorage {
    type Error = Infallible;
    type Text<'a> = Box<str>;
    type Contiguous<'a, T: 'a> = Box<[T]>;
    type Map<'a, V: 'a> = BTreeMap<Box<str>, V>;

    fn try_text<'a>(&'a self, value: &str) -> Result<Self::Text<'a>, Self::Error> {
        Ok(Box::from(value))
    }

    fn try_contiguous<'a, T: 'a>(
        &'a self,
        values: impl IntoIterator<Item = T>,
    ) -> Result<Self::Contiguous<'a, T>, Self::Error> {
        Ok(values.into_iter().collect::<Vec<_>>().into_boxed_slice())
    }

    fn try_map<'a, V: 'a>(
        &'a self,
        entries: impl IntoIterator<Item = (Self::Text<'a>, V)>,
    ) -> Result<Self::Map<'a, V>, Self::Error> {
        Ok(entries.into_iter().collect())
    }
}

/// Tree-map access with logarithmic lookup by text.
///
/// The key's `Borrow<str>` and `AsRef<str>` representations must agree, and its
/// ordering must match the borrowed text, as required by `Borrow`.
impl<K: AsRef<str> + Borrow<str> + Ord, V> Map for BTreeMap<K, V> {
    type Key = K;
    type Value = V;

    fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.iter()
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn get(&self, key: &str) -> Option<&V> {
        self.get(key)
    }
}
