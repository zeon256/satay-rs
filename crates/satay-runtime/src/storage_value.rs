//! Optional value operations for storage families.
use crate::{
    StringPolicy, StringStorage,
    storage::{AllocStorage, BoxedStorage, Storage},
};
use core::fmt;
use std::borrow;

/// A family that can clone values without borrowing a construction context.
///
/// This is separate from `Storage`: arena policies and read-only containers do
/// not have to offer cloning. The element callback avoids recursive container
/// trait bounds in generated recursive models.
pub trait CloneStorage: Storage {
    /// Clones stored text, retaining its storage lifetime.
    fn clone_text<'a>(value: &Self::Text<'a>) -> Self::Text<'a>;
    /// Clones a collection with a schema-aware callback for each element.
    fn clone_contiguous<'a, T: 'a>(
        value: &Self::Contiguous<'a, T>,
        clone: impl FnMut(&T) -> T,
    ) -> Self::Contiguous<'a, T>;
}
impl CloneStorage for AllocStorage {
    fn clone_text<'a>(value: &Self::Text<'a>) -> Self::Text<'a> {
        value.clone()
    }
    fn clone_contiguous<'a, T: 'a>(
        value: &Self::Contiguous<'a, T>,
        clone: impl FnMut(&T) -> T,
    ) -> Self::Contiguous<'a, T> {
        value.iter().map(clone).collect()
    }
}
impl CloneStorage for BoxedStorage {
    fn clone_text<'a>(value: &Self::Text<'a>) -> Self::Text<'a> {
        value.clone()
    }
    fn clone_contiguous<'a, T: 'a>(
        value: &Self::Contiguous<'a, T>,
        clone: impl FnMut(&T) -> T,
    ) -> Self::Contiguous<'a, T> {
        value.iter().map(clone).collect()
    }
}
impl<T: StringStorage + borrow::Borrow<str> + 'static> CloneStorage for StringPolicy<T> {
    fn clone_text<'a>(value: &Self::Text<'a>) -> Self::Text<'a> {
        value.clone()
    }
    fn clone_contiguous<'a, V: 'a>(
        value: &Self::Contiguous<'a, V>,
        clone: impl FnMut(&V) -> V,
    ) -> Self::Contiguous<'a, V> {
        value.iter().map(clone).collect()
    }
}

/// Debug view using a schema-aware formatter rather than the container's Debug.
pub struct DebugWith<T, F> {
    value: T,
    format: F,
}
impl<T, F> DebugWith<T, F> {
    /// Borrows a value and supplies its formatter without allocating.
    pub const fn new(value: T, format: F) -> Self {
        Self { value, format }
    }
}
impl<T, F: Fn(&T, &mut fmt::Formatter<'_>) -> fmt::Result> fmt::Debug for DebugWith<T, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.format)(&self.value, f)
    }
}
