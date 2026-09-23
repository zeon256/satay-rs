#![cfg(feature = "json")]

use std::{collections::BTreeMap, convert::Infallible, fmt};

use bumpalo::{
    Bump,
    collections::{String as BumpString, Vec as BumpVec},
};
use satay_runtime::{
    Error,
    storage::{AllocStorage, BoxedStorage, Storage},
    storage_serde::{
        CollectionStorage, ContiguousSeed, DecodeContext, DecodeError, TextSeed, ValueSeed,
        from_json_slice_in,
    },
};
use serde::de::value;
use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeSeed, Error as DeError, MapAccess, SeqAccess, Visitor},
};

// Intentionally has no Clone, Debug, or Serde implementations.
struct Arena(Bump);
impl Storage for Arena {
    type Error = Infallible;
    type Text<'a> = BumpString<'a>;
    type Contiguous<'a, T: 'a> = BumpVec<'a, T>;
    type Map<'a, V: 'a> = BTreeMap<BumpString<'a>, V>;
    fn try_text<'a>(&'a self, value: &str) -> Result<Self::Text<'a>, Self::Error> {
        Ok(BumpString::from_str_in(value, &self.0))
    }
    fn try_contiguous<'a, T: 'a>(
        &'a self,
        values: impl IntoIterator<Item = T>,
    ) -> Result<Self::Contiguous<'a, T>, Self::Error> {
        Ok(BumpVec::from_iter_in(values, &self.0))
    }
    fn try_map<'a, V: 'a>(
        &'a self,
        values: impl IntoIterator<Item = (Self::Text<'a>, V)>,
    ) -> Result<Self::Map<'a, V>, Self::Error> {
        Ok(values.into_iter().collect())
    }
}
impl CollectionStorage for Arena {
    fn deserialize_contiguous<'storage, 'de, T: 'storage, A, E>(
        context: &DecodeContext<'storage, Self>,
        mut sequence: A,
        element: E,
    ) -> Result<Self::Contiguous<'storage, T>, A::Error>
    where
        A: SeqAccess<'de>,
        E: DeserializeSeed<'de, Value = T> + Clone,
    {
        let mut values = BumpVec::new_in(&context.storage().0);
        while let Some(value) = sequence.next_element_seed(element.clone())? {
            values.push(value);
        }
        Ok(values)
    }
}

type Counts<'storage, S = AllocStorage> = <S as Storage>::Contiguous<'storage, i64>;
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(bound(
    serialize = "S::Text<'storage>: serde::Serialize, Counts<'storage, S>: serde::Serialize",
    deserialize = "S::Text<'storage>: serde::Deserialize<'de>, Counts<'storage, S>: serde::Deserialize<'de>"
))]
struct Pet<'storage, S: Storage + 'storage = AllocStorage> {
    name: S::Text<'storage>,
    counts: Counts<'storage, S>,
}
impl<'storage, S: Storage> Clone for Pet<'storage, S>
where
    S::Text<'storage>: Clone,
    Counts<'storage, S>: Clone,
{
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            counts: self.counts.clone(),
        }
    }
}
impl<'storage, S: Storage> fmt::Debug for Pet<'storage, S>
where
    S::Text<'storage>: fmt::Debug,
    Counts<'storage, S>: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pet")
            .field("name", &self.name)
            .field("counts", &self.counts)
            .finish()
    }
}
impl<'storage, S: Storage> PartialEq for Pet<'storage, S>
where
    S::Text<'storage>: PartialEq,
    Counts<'storage, S>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.counts == other.counts
    }
}
#[path = "storage_serde/owned.rs"]
mod owned;

struct PetSeed<'ctx, 'storage, S: Storage>(&'ctx DecodeContext<'storage, S>);
impl<S: Storage> Copy for PetSeed<'_, '_, S> {}
impl<S: Storage> Clone for PetSeed<'_, '_, S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'storage, 'de, S: CollectionStorage> DeserializeSeed<'de> for PetSeed<'_, 'storage, S> {
    type Value = Pet<'storage, S>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'storage, 'de, S: CollectionStorage> Visitor<'de> for PetSeed<'_, 'storage, S> {
    type Value = Pet<'storage, S>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a pet")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let (mut name, mut counts) = (None, None);
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "name" => {
                    if name.is_some() {
                        return Err(DeError::duplicate_field("name"));
                    }
                    name = Some(map.next_value_seed(TextSeed(self.0))?);
                }
                "counts" => {
                    if counts.is_some() {
                        return Err(DeError::duplicate_field("counts"));
                    }
                    counts = Some(map.next_value_seed(ContiguousSeed {
                        context: self.0,
                        element: ValueSeed::<i64>::new(),
                    })?);
                }
                _ => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }
        Ok(Pet {
            name: name.ok_or_else(|| DeError::missing_field("name"))?,
            counts: counts.ok_or_else(|| DeError::missing_field("counts"))?,
        })
    }
}

#[test]
fn nested_models_use_one_arena_and_outlive_input() {
    let arena = Arena(Bump::new());
    let pets = {
        let bytes = br#"[{"name":"Milo", "counts":[1,2,3]}]"#.to_vec();
        from_json_slice_in(&arena, &bytes, |context, deserializer| {
            ContiguousSeed {
                context,
                element: PetSeed(context),
            }
            .deserialize(deserializer)
        })
        .unwrap()
    };
    assert_eq!(pets[0].name.as_str(), "Milo");
    assert_eq!(pets[0].counts.as_slice(), [1, 2, 3]);
    assert_eq!(pets[0], pets[0].clone());
}

#[test]
fn owned_aliases_and_boxed_values_do_not_retain_contexts() {
    let pet: owned::Pet = {
        let policy = AllocStorage;
        Pet {
            name: policy.try_text("Milo").unwrap(),
            counts: policy.try_contiguous([3, 2, 1]).unwrap(),
        }
    };
    assert_eq!(pet.counts, [3, 2, 1]);
    let pet: Pet<'static, BoxedStorage> = {
        let policy = BoxedStorage;
        Pet {
            name: policy.try_text("Milo").unwrap(),
            counts: policy.try_contiguous([3, 2, 1]).unwrap(),
        }
    };
    let _: Box<str> = pet.name.clone();
    let _: Box<[i64]> = pet.counts.clone();
    let bytes = serde_json::to_vec(&pet).unwrap();
    let decoded: Pet<'static, BoxedStorage> = serde_json::from_slice(&bytes).unwrap();
    drop(bytes);
    assert_eq!(decoded, pet);
}

// The error deliberately lacks Display and Error implementations.
#[derive(Debug, PartialEq)]
enum Exhausted {
    Text,
    Collection,
}
struct Limited;
impl Storage for Limited {
    type Error = Exhausted;
    type Text<'a> = String;
    type Contiguous<'a, T: 'a> = Vec<T>;
    type Map<'a, V: 'a> = BTreeMap<String, V>;
    fn try_text<'a>(&'a self, _: &str) -> Result<Self::Text<'a>, Self::Error> {
        Err(Exhausted::Text)
    }
    fn try_contiguous<'a, T: 'a>(
        &'a self,
        _: impl IntoIterator<Item = T>,
    ) -> Result<Self::Contiguous<'a, T>, Self::Error> {
        Err(Exhausted::Collection)
    }
    fn try_map<'a, V: 'a>(
        &'a self,
        values: impl IntoIterator<Item = (Self::Text<'a>, V)>,
    ) -> Result<Self::Map<'a, V>, Self::Error> {
        Ok(values.into_iter().collect())
    }
}
impl CollectionStorage for Limited {
    fn deserialize_contiguous<'storage, 'de, T: 'storage, A, E>(
        context: &DecodeContext<'storage, Self>,
        mut sequence: A,
        element: E,
    ) -> Result<Self::Contiguous<'storage, T>, A::Error>
    where
        A: SeqAccess<'de>,
        E: DeserializeSeed<'de, Value = T> + Clone,
    {
        let mut values = vec![];
        while let Some(value) = sequence.next_element_seed(element.clone())? {
            values.push(value);
        }
        context
            .storage()
            .try_contiguous(values)
            .map_err(|error| context.storage_error(error))
    }
}

#[test]
fn errors_preserve_storage_failures_and_never_return_partial_sequences() {
    let result = from_json_slice_in(&Limited, br#"["ok"]"#, |context, deserializer| {
        ContiguousSeed {
            context,
            element: TextSeed(context),
        }
        .deserialize(deserializer)
    });
    assert!(matches!(result, Err(DecodeError::Storage(Exhausted::Text))));
    let result = from_json_slice_in(&Limited, b"[1,2]", |context, deserializer| {
        ContiguousSeed {
            context,
            element: ValueSeed::<i64>::new(),
        }
        .deserialize(deserializer)
    });
    assert!(matches!(
        result,
        Err(DecodeError::Storage(Exhausted::Collection))
    ));
    for bytes in [b"[1,false]".as_slice(), b"[1,", b"[1] true"] {
        let result = from_json_slice_in(&AllocStorage, bytes, |context, deserializer| {
            ContiguousSeed {
                context,
                element: ValueSeed::<i64>::new(),
            }
            .deserialize(deserializer)
        });
        assert!(matches!(result, Err(DecodeError::Decode(_))));
    }
    let result = from_json_slice_in(&Limited, br#""text""#, |context, deserializer| {
        // Simulate lossy-field or untagged-union code catching a Serde error.
        let _ = TextSeed(context).deserialize(deserializer);
        Ok(())
    });
    assert!(matches!(result, Err(DecodeError::Storage(Exhausted::Text))));
}

#[test]
fn nested_maps_options_and_lossy_replay_keep_the_same_context() {
    use satay_runtime::storage_serde::{MapSeed, OptionSeed, deserialize_lossy};
    let arena = Arena(Bump::new());
    let values = {
        let bytes = br#"{"some":["Milo",null,"cat"],"empty":[]}"#.to_vec();
        from_json_slice_in(&arena, &bytes, |context, deserializer| {
            MapSeed(ContiguousSeed {
                context,
                element: OptionSeed(TextSeed(context)),
            })
            .deserialize(deserializer)
        })
        .unwrap()
    };
    assert_eq!(values["some"][0].as_ref().unwrap().as_str(), "Milo");
    assert!(values["some"][1].is_none());
    assert!(values["empty"].is_empty());
    let text = from_json_slice_in(&arena, br#""copied""#, |context, deserializer| {
        deserialize_lossy(context, TextSeed(context), deserializer)
    })
    .unwrap()
    .unwrap();
    assert_eq!(text.as_str(), "copied");
    let invalid = from_json_slice_in(&arena, b"false", |context, deserializer| {
        deserialize_lossy(context, TextSeed(context), deserializer)
    })
    .unwrap();
    assert!(invalid.is_none());
    let exhausted = from_json_slice_in(&Limited, br#""copied""#, |context, deserializer| {
        deserialize_lossy(context, TextSeed(context), deserializer)
    });
    assert!(matches!(
        exhausted,
        Err(DecodeError::Storage(Exhausted::Text))
    ));
}

#[test]
fn sequence_size_hints_are_not_lengths_or_allocation_requests() {
    struct Sequence {
        remaining: usize,
        hint: usize,
    }
    impl<'de> SeqAccess<'de> for Sequence {
        type Error = value::Error;
        fn next_element_seed<E: DeserializeSeed<'de>>(
            &mut self,
            seed: E,
        ) -> Result<Option<E::Value>, Self::Error> {
            if self.remaining == 0 {
                return Ok(None);
            }
            self.remaining -= 1;
            seed.deserialize(value::I64Deserializer::new(42)).map(Some)
        }
        fn size_hint(&self) -> Option<usize> {
            Some(self.hint)
        }
    }
    for hint in [0, 1, usize::MAX] {
        let context = DecodeContext::new(&AllocStorage);
        let values = AllocStorage::deserialize_contiguous(
            &context,
            Sequence { remaining: 3, hint },
            ValueSeed::<i64>::new(),
        )
        .unwrap();
        assert_eq!(values, [42, 42, 42]);
        let context = DecodeContext::new(&BoxedStorage);
        let values = BoxedStorage::deserialize_contiguous(
            &context,
            Sequence { remaining: 3, hint },
            ValueSeed::<i64>::new(),
        )
        .unwrap();
        assert_eq!(values.as_ref(), [42, 42, 42]);
        let arena = Arena(Bump::new());
        let context = DecodeContext::new(&arena);
        let values = Arena::deserialize_contiguous(
            &context,
            Sequence { remaining: 3, hint },
            ValueSeed::<i64>::new(),
        )
        .unwrap();
        assert_eq!(values.as_slice(), [42, 42, 42]);
    }
}

#[test]
fn readonly_contiguous_containers_serialize_via_slices() {
    use satay_runtime::storage_serde::serialize_contiguous;
    struct ReadOnly<T>(Box<[T]>);
    impl<T> AsRef<[T]> for ReadOnly<T> {
        fn as_ref(&self) -> &[T] {
            &self.0
        }
    }
    #[derive(serde::Serialize)]
    struct Model {
        #[serde(serialize_with = "serialize_contiguous")]
        values: ReadOnly<i64>,
    }
    let model = Model {
        values: ReadOnly(vec![2, 1, 3].into_boxed_slice()),
    };
    assert_eq!(
        serde_json::to_string(&model).unwrap(),
        r#"{"values":[2,1,3]}"#
    );
}

#[test]
fn projection_replays_into_arena_and_preserves_storage_errors() {
    use satay_runtime::storage_serde::from_projected_json_slice_in;
    let arena = Arena(Bump::new());
    let names = {
        let bytes = br#"{"items":[{"name":"Milo"},{"name":"Otis"}]}"#.to_vec();
        from_projected_json_slice_in(&arena, &bytes, "items", Some("name"), |context, value| {
            ContiguousSeed {
                context,
                element: TextSeed(context),
            }
            .deserialize(value)
        })
        .unwrap()
    };
    assert_eq!(names[0].as_str(), "Milo");
    assert_eq!(names[1].as_str(), "Otis");
    let result = from_projected_json_slice_in(
        &Limited,
        br#"{"name":"Milo"}"#,
        "name",
        None,
        |context, value| TextSeed(context).deserialize(value),
    );
    assert!(matches!(result, Err(DecodeError::Storage(Exhausted::Text))));
    let result = from_projected_json_slice_in(&arena, b"[]", "name", None, |context, value| {
        TextSeed(context).deserialize(value)
    });
    assert!(matches!(
        result,
        Err(DecodeError::Decode(Error::InvalidResponse(_)))
    ));
}

#[test]
fn failed_elements_drop_previously_constructed_elements() {
    use std::{cell::Cell, rc::Rc};
    struct Tracked(Rc<Cell<usize>>);
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    #[derive(Clone)]
    struct TrackedSeed(Rc<Cell<usize>>);
    impl<'de> DeserializeSeed<'de> for TrackedSeed {
        type Value = Tracked;
        fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Tracked, D::Error> {
            i64::deserialize(deserializer)?;
            Ok(Tracked(self.0))
        }
    }
    let dropped = Rc::new(Cell::new(0));
    let arena = Arena(Bump::new());
    let result = from_json_slice_in(&arena, b"[1,2,false]", |context, deserializer| {
        ContiguousSeed {
            context,
            element: TrackedSeed(dropped.clone()),
        }
        .deserialize(deserializer)
    });
    assert!(matches!(result, Err(DecodeError::Decode(_))));
    assert_eq!(dropped.get(), 2);
}
