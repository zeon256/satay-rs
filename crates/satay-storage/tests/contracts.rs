//! Consumer tests: bounded storage uses neither allocation nor key trait bounds.
#[cfg(feature = "alloc")]
use core::fmt;
use core::str;

use satay_storage::{Map, Storage};

// Deliberately implements neither Eq, Ord, Hash, nor Clone.
struct Text {
    bytes: [u8; 8],
    len: usize,
}

impl AsRef<str> for Text {
    fn as_ref(&self) -> &str {
        str::from_utf8(&self.bytes[..self.len]).unwrap()
    }
}

struct One<T>([T; 1]);

impl<T> AsRef<[T]> for One<T> {
    fn as_ref(&self) -> &[T] {
        &self.0
    }
}

struct Entry<V>(Option<(Text, V)>);

impl<V> Map for Entry<V> {
    type Key = Text;
    type Value = V;

    fn iter(&self) -> impl Iterator<Item = (&Text, &V)> {
        self.0.iter().map(|(key, value)| (key, value))
    }

    fn len(&self) -> usize {
        usize::from(self.0.is_some())
    }
}

#[derive(Debug, PartialEq)]
enum CapacityError {
    TextTooLong,
    ExpectedOne,
    TooManyKeys,
}

struct Bounded;

impl Storage for Bounded {
    type Error = CapacityError;
    type Text<'a> = Text;
    type Contiguous<'a, T: 'a> = One<T>;
    type Map<'a, V: 'a> = Entry<V>;

    fn try_text<'a>(&'a self, value: &str) -> Result<Self::Text<'a>, Self::Error> {
        let mut bytes = [0; 8];
        if value.len() > bytes.len() {
            return Err(CapacityError::TextTooLong);
        }
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Ok(Text {
            bytes,
            len: value.len(),
        })
    }

    fn try_contiguous<'a, T: 'a>(
        &'a self,
        values: impl IntoIterator<Item = T>,
    ) -> Result<Self::Contiguous<'a, T>, Self::Error> {
        let mut values = values.into_iter();
        let first = values.next().ok_or(CapacityError::ExpectedOne)?;
        if values.next().is_some() {
            return Err(CapacityError::ExpectedOne);
        }
        Ok(One([first]))
    }

    fn try_map<'a, V: 'a>(
        &'a self,
        entries: impl IntoIterator<Item = (Self::Text<'a>, V)>,
    ) -> Result<Self::Map<'a, V>, Self::Error> {
        let mut result = Entry::<V>(None);
        for (key, value) in entries {
            if let Some((stored, _)) = &result.0
                && stored.as_ref() != key.as_ref()
            {
                return Err(CapacityError::TooManyKeys);
            }
            result.0 = Some((key, value));
        }
        Ok(result)
    }
}

#[test]
fn bounded_storage_supports_non_orderable_keys_and_borrowed_values() {
    let storage = Bounded;
    let value = 42;
    let items = storage.try_contiguous([&value]).unwrap();
    assert_eq!(items.as_ref(), &[&42]);
    let entries = storage
        .try_map([
            (storage.try_text("café").unwrap(), 1),
            (storage.try_text("café").unwrap(), 2),
        ])
        .unwrap();
    assert_eq!(entries.get("café"), Some(&2));
    assert_eq!(entries.get("missing"), None);
    assert_eq!(entries.len(), 1);
    assert!(!entries.is_empty());
    assert!(storage.try_map::<()>([]).unwrap().is_empty());
}

#[test]
fn construction_errors_are_returned() {
    let storage = Bounded;
    assert!(matches!(
        storage.try_text("too many bytes"),
        Err(CapacityError::TextTooLong)
    ));
    assert!(matches!(
        storage.try_contiguous([1, 2]),
        Err(CapacityError::ExpectedOne)
    ));
    assert!(matches!(
        storage.try_contiguous::<()>([]),
        Err(CapacityError::ExpectedOne)
    ));
    assert!(matches!(
        storage.try_map([
            (storage.try_text("a").unwrap(), 1),
            (storage.try_text("b").unwrap(), 2),
        ]),
        Err(CapacityError::TooManyKeys)
    ));
}

#[cfg(feature = "alloc")]
#[test]
fn owned_policies_obey_collection_and_map_contracts() {
    fn check<S: Storage>(storage: &S)
    where
        S::Error: fmt::Debug,
    {
        let values = storage.try_contiguous([3, 1, 2]).unwrap();
        assert_eq!(values.as_ref(), &[3, 1, 2]);
        assert!(
            storage
                .try_contiguous::<()>([])
                .unwrap()
                .as_ref()
                .is_empty()
        );
        let map = storage
            .try_map([
                (storage.try_text("z").unwrap(), 1),
                (storage.try_text("a").unwrap(), 2),
                (storage.try_text("z").unwrap(), 3),
            ])
            .unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("z"), Some(&3));
        assert_eq!(map.get("missing"), None);
        assert_eq!(
            map.iter().map(|(key, _)| key.as_ref()).collect::<Vec<_>>(),
            ["a", "z"]
        );
        assert!(storage.try_map::<()>([]).unwrap().is_empty());
    }
    check(&satay_storage::AllocStorage);
    check(&satay_storage::BoxedStorage);
}

#[cfg(feature = "alloc")]
#[test]
fn all_owned_representations_outlive_the_context() {
    let (text, values, entries) = {
        let storage = satay_storage::AllocStorage;
        (
            storage.try_text("cat").unwrap(),
            storage.try_contiguous([1]).unwrap(),
            storage
                .try_map([(storage.try_text("cat").unwrap(), 1)])
                .unwrap(),
        )
    };
    assert_eq!(text, "cat");
    assert_eq!(values, [1]);
    assert_eq!(entries.get("cat"), Some(&1));
    let (text, values, entries) = {
        let storage = satay_storage::BoxedStorage;
        (
            storage.try_text("cat").unwrap(),
            storage.try_contiguous([1]).unwrap(),
            storage
                .try_map([(storage.try_text("cat").unwrap(), 1)])
                .unwrap(),
        )
    };
    assert_eq!(text.as_ref(), "cat");
    assert_eq!(values.as_ref(), &[1]);
    assert_eq!(entries.get("cat"), Some(&1));
}
