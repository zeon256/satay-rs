use std::{collections::BTreeMap, convert::Infallible};
use bumpalo::{Bump, collections::{String as BumpString, Vec as BumpVec}};
use satay_runtime::{storage::Storage, storage_serde::{TextSeed, from_json_slice_in}};
use serde::de::DeserializeSeed;

struct Arena(Bump);
impl Storage for Arena {
    type Error = Infallible;
    type Text<'a> = BumpString<'a>;
    type Contiguous<'a, T: 'a> = BumpVec<'a, T>;
    type Map<'a, V: 'a> = BTreeMap<BumpString<'a>, V>;
    fn try_text<'a>(&'a self, value: &str) -> Result<Self::Text<'a>, Self::Error> {
        Ok(BumpString::from_str_in(value, &self.0))
    }
    fn try_contiguous<'a, T: 'a>(&'a self, values: impl IntoIterator<Item = T>) -> Result<Self::Contiguous<'a, T>, Self::Error> {
        Ok(BumpVec::from_iter_in(values, &self.0))
    }
    fn try_map<'a, V: 'a>(&'a self, values: impl IntoIterator<Item = (Self::Text<'a>, V)>) -> Result<Self::Map<'a, V>, Self::Error> {
        Ok(values.into_iter().collect())
    }
}

struct Pet<'storage, S: Storage + 'storage> {
    name: S::Text<'storage>,
}

fn decode_pet<'storage>(arena: &'storage Arena, bytes: &[u8]) -> Pet<'storage, Arena> {
    Pet { name: from_json_slice_in(arena, bytes, |context, deserializer| TextSeed(context).deserialize(deserializer)).unwrap() }
}
