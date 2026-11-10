//! Storage policies a consumer of generated code might write.
//!
//! Each implements only the storage contract: no Serde, `Debug`, `Clone`, or
//! comparison support is required of the policy or its containers.

use std::collections::BTreeMap;
use std::convert::Infallible;

use bumpalo::Bump;
use bumpalo::collections::{String as BumpString, Vec as BumpVec};
use compact_str::CompactString;
use satay_runtime::{OwnedStorage, Storage};

/// An owned policy with inline small strings.
pub struct Compact;

impl Storage for Compact {
    type Error = Infallible;
    type Text<'a> = CompactString;
    type Contiguous<'a, T: 'a> = Vec<T>;
    type Map<'a, V: 'a> = BTreeMap<CompactString, V>;

    fn try_text<'a>(&'a self, value: &str) -> Result<Self::Text<'a>, Self::Error> {
        Ok(CompactString::from(value))
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

impl OwnedStorage for Compact {
    const CONTEXT: &'static Self = &Self;
}

/// An arena policy: text and collections borrow from the bump allocator.
#[derive(Default)]
pub struct Arena(pub Bump);

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
        entries: impl IntoIterator<Item = (Self::Text<'a>, V)>,
    ) -> Result<Self::Map<'a, V>, Self::Error> {
        Ok(entries.into_iter().collect())
    }
}
