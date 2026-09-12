//! A downstream std policy. Both external traits need local wrapper types.
use std::{collections::HashMap, convert::Infallible};

use satay_storage::{Map, Storage};

struct HashEntries<V>(HashMap<String, V>);

impl<V> Map for HashEntries<V> {
    type Key = String;
    type Value = V;

    fn iter(&self) -> impl Iterator<Item = (&String, &V)> {
        self.0.iter()
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn get(&self, key: &str) -> Option<&V> {
        self.0.get(key)
    }
}

struct HashStorage;

impl Storage for HashStorage {
    type Error = Infallible;
    type Text<'a> = String;
    type Contiguous<'a, T: 'a> = Vec<T>;
    type Map<'a, V: 'a> = HashEntries<V>;

    fn try_text<'a>(&'a self, value: &str) -> Result<Self::Text<'a>, Self::Error> {
        Ok(value.into())
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
        Ok(HashEntries(entries.into_iter().collect()))
    }
}

fn main() -> Result<(), Infallible> {
    let storage = HashStorage;
    let entries = storage.try_map([
        (storage.try_text("cat")?, 1),
        (storage.try_text("dog")?, 2),
        (storage.try_text("cat")?, 3),
    ])?;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries.get("cat"), Some(&3));
    assert_eq!(entries.iter().count(), 2);
    Ok(())
}
