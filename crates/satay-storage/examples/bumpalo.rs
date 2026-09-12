//! Downstream arena policy: text/elements use the arena; map nodes use alloc.
extern crate alloc;

use alloc::{collections::BTreeMap, string::String};
use core::convert::Infallible;

use bumpalo::{
    Bump,
    collections::{String as BumpString, Vec as BumpVec},
};
use satay_storage::{Map, Storage};

#[derive(Debug, Default)]
struct ArenaStorage(Bump);

impl Storage for ArenaStorage {
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
        entries: impl IntoIterator<Item = (Self::Text<'a>, V)>,
    ) -> Result<Self::Map<'a, V>, Self::Error> {
        Ok(entries.into_iter().collect())
    }
}

struct Pet<'a, S: Storage + 'a> {
    name: S::Text<'a>,
    tags: S::Contiguous<'a, S::Text<'a>>,
}

#[allow(
    clippy::mutable_key_type,
    reason = "BumpString ordering depends on its text, not the arena's interior state"
)]
fn main() -> Result<(), Infallible> {
    let storage = ArenaStorage::default();
    let name = {
        let input = String::from("Milo");
        storage.try_text(&input)? // Input may be dropped; storage must stay alive.
    };
    let pets = storage.try_contiguous([Pet::<'_, ArenaStorage> {
        name,
        tags: storage.try_contiguous([storage.try_text("cat")?])?,
    }])?;
    let map = storage.try_map([(storage.try_text("pets")?, pets)])?;
    let pets = Map::get(&map, "pets").unwrap();
    assert_eq!(pets[0].name.as_str(), "Milo");
    assert_eq!(pets[0].tags[0].as_str(), "cat");
    Ok(())
}
