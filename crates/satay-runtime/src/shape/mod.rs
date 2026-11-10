//! Derives generated-model behavior from the [`Storage`] contract alone.
//!
//! Generated models store `S::Text<'s>`, `S::Contiguous<'s, T>`, and
//! `S::Map<'s, V>` directly, so the default policy keeps plain `String`,
//! `Vec<T>`, and `BTreeMap<String, V>` fields. Those projections promise only
//! read access and construction through a storage context. A shape marker names
//! how one field is assembled from them, and the traits in this module supply
//! formatting, equality, cloning, and codecs for that field. Models therefore
//! bound their storage parameter by [`Storage`] and nothing else: a policy never
//! implements Serde, `Debug`, or `PartialEq` for its containers.
//!
//! | Marker | Field type |
//! | --- | --- |
//! | [`Text`] | `S::Text<'s>` |
//! | [`Contiguous<E>`] | `S::Contiguous<'s, E::Value>` |
//! | [`Map<E>`] | `S::Map<'s, E::Value>` |
//! | [`Optional<E>`] | `Option<E::Value>` |
//! | [`Plain<T>`] | `T`, independent of the storage policy |
//! | [`Model<T>`] | `T`, a generated model generic over the policy |

use std::convert::Infallible;
use std::fmt::{self, Debug};
use std::marker::PhantomData;

use satay_storage::{Map as Entries, OwnedStorage, Storage};

#[cfg(feature = "serde")]
mod buffer;
#[cfg(feature = "serde")]
mod codec;
#[cfg(feature = "json")]
mod json;

#[cfg(feature = "serde")]
pub use buffer::Buffered;
#[cfg(feature = "serde")]
pub use codec::{
    Decode, DecodeIn, Encode, Field, Seed, deserialize, required, seed, serialize, text, vacant,
};
#[cfg(feature = "json")]
pub use json::{
    Lossy, from_json_slice, from_projected_json_slice, into_json_request,
    into_optional_json_request, lossy,
};

/// Maps a marker to the field type it describes under policy `S`.
pub trait Shape<'s, S: Storage + 's> {
    /// The described field type.
    type Value: 's;
}

/// Text owned by the storage policy.
#[derive(Debug)]
pub struct Text(Infallible);

/// A contiguous collection of `E` values owned by the storage policy.
#[derive(Debug)]
pub struct Contiguous<E>(PhantomData<E>, Infallible);

/// A text-keyed map of `E` values owned by the storage policy.
#[derive(Debug)]
pub struct Map<E>(PhantomData<E>, Infallible);

/// An optional `E` value.
#[derive(Debug)]
pub struct Optional<E>(PhantomData<E>, Infallible);

/// A value whose representation does not depend on the storage policy.
#[derive(Debug)]
pub struct Plain<T>(PhantomData<T>, Infallible);

/// A generated model that is itself generic over the storage policy.
#[derive(Debug)]
pub struct Model<T>(PhantomData<T>, Infallible);

impl<'s, S: Storage + 's> Shape<'s, S> for Text {
    type Value = S::Text<'s>;
}

impl<'s, S: Storage + 's, E: Shape<'s, S>> Shape<'s, S> for Contiguous<E> {
    type Value = S::Contiguous<'s, E::Value>;
}

impl<'s, S: Storage + 's, E: Shape<'s, S>> Shape<'s, S> for Map<E> {
    type Value = S::Map<'s, E::Value>;
}

impl<'s, S: Storage + 's, E: Shape<'s, S>> Shape<'s, S> for Optional<E> {
    type Value = Option<E::Value>;
}

impl<'s, S: Storage + 's, T: 's> Shape<'s, S> for Plain<T> {
    type Value = T;
}

impl<'s, S: Storage + 's, T: 's> Shape<'s, S> for Model<T> {
    type Value = T;
}

/// Formatting and equality through the storage read contract.
pub trait Inspect<'s, S: Storage + 's>: Shape<'s, S> {
    /// Formats the value as its default-policy representation would.
    ///
    /// # Errors
    /// Propagates errors from the formatter.
    fn fmt(value: &Self::Value, f: &mut fmt::Formatter<'_>) -> fmt::Result;

    /// Compares text, elements, and entries. Map entry order is not significant.
    fn eq(left: &Self::Value, right: &Self::Value) -> bool;
}

/// Borrows a field so it formats and serializes through its shape.
pub struct View<'v, 's, Sh: Shape<'s, S>, S: Storage + 's>(&'v Sh::Value);

/// Borrows `value` as shape `Sh`.
pub fn view<'v, 's, Sh: Shape<'s, S>, S: Storage + 's>(
    value: &'v Sh::Value,
) -> View<'v, 's, Sh, S> {
    View(value)
}

/// Compares two values of shape `Sh`.
pub fn eq<'s, Sh: Inspect<'s, S>, S: Storage + 's>(left: &Sh::Value, right: &Sh::Value) -> bool {
    Sh::eq(left, right)
}

impl<'s, Sh: Inspect<'s, S>, S: Storage + 's> fmt::Debug for View<'_, 's, Sh, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Sh::fmt(self.0, f)
    }
}

impl<'s, S: Storage + 's> Inspect<'s, S> for Text {
    fn fmt(value: &Self::Value, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(value.as_ref(), f)
    }

    fn eq(left: &Self::Value, right: &Self::Value) -> bool {
        left.as_ref() == right.as_ref()
    }
}

impl<'s, S: Storage + 's, E: Inspect<'s, S>> Inspect<'s, S> for Contiguous<E> {
    fn fmt(value: &Self::Value, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(value.as_ref().iter().map(view::<E, S>))
            .finish()
    }

    fn eq(left: &Self::Value, right: &Self::Value) -> bool {
        let (left, right) = (left.as_ref(), right.as_ref());
        left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(left, right)| E::eq(left, right))
    }
}

impl<'s, S: Storage + 's, E: Inspect<'s, S>> Inspect<'s, S> for Map<E> {
    fn fmt(value: &Self::Value, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(
                value
                    .iter()
                    .map(|(key, value)| (key.as_ref(), view::<E, S>(value))),
            )
            .finish()
    }

    fn eq(left: &Self::Value, right: &Self::Value) -> bool {
        left.len() == right.len()
            && left.iter().all(|(key, left)| {
                right
                    .get(key.as_ref())
                    .is_some_and(|right| E::eq(left, right))
            })
    }
}

impl<'s, S: Storage + 's, E: Inspect<'s, S>> Inspect<'s, S> for Optional<E> {
    fn fmt(value: &Self::Value, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&value.as_ref().map(view::<E, S>), f)
    }

    fn eq(left: &Self::Value, right: &Self::Value) -> bool {
        match (left, right) {
            (Some(left), Some(right)) => E::eq(left, right),
            (None, None) => true,
            _ => false,
        }
    }
}

/// Cloning for policies that construct values without a context or failure.
pub trait Duplicate<'s, S: OwnedStorage<Error = Infallible>>: Shape<'s, S> {
    /// Rebuilds the value through [`OwnedStorage::CONTEXT`].
    fn duplicate(value: &Self::Value) -> Self::Value;
}

/// Clones a value of shape `Sh`.
pub fn duplicate<'s, Sh: Duplicate<'s, S>, S: OwnedStorage<Error = Infallible>>(
    value: &Sh::Value,
) -> Sh::Value {
    Sh::duplicate(value)
}

/// Stores `value` as policy text when the policy needs neither context nor fallibility.
pub fn owned_text<'s, S: OwnedStorage<Error = Infallible>>(value: &str) -> S::Text<'s> {
    infallible(S::CONTEXT.try_text(value))
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

impl<'s, S: OwnedStorage<Error = Infallible>> Duplicate<'s, S> for Text {
    fn duplicate(value: &Self::Value) -> Self::Value {
        owned_text::<S>(value.as_ref())
    }
}

impl<'s, S: OwnedStorage<Error = Infallible>, E: Duplicate<'s, S>> Duplicate<'s, S>
    for Contiguous<E>
{
    fn duplicate(value: &Self::Value) -> Self::Value {
        infallible(S::CONTEXT.try_contiguous(value.as_ref().iter().map(E::duplicate)))
    }
}

impl<'s, S: OwnedStorage<Error = Infallible>, E: Duplicate<'s, S>> Duplicate<'s, S> for Map<E> {
    fn duplicate(value: &Self::Value) -> Self::Value {
        infallible(
            S::CONTEXT.try_map(
                value
                    .iter()
                    .map(|(key, value)| (owned_text::<S>(key.as_ref()), E::duplicate(value))),
            ),
        )
    }
}

impl<'s, S: OwnedStorage<Error = Infallible>, E: Duplicate<'s, S>> Duplicate<'s, S>
    for Optional<E>
{
    fn duplicate(value: &Self::Value) -> Self::Value {
        value.as_ref().map(E::duplicate)
    }
}

macro_rules! leaf {
    ($($marker:ident),*) => {$(
        impl<'s, S: Storage + 's, T: fmt::Debug + PartialEq + 's> Inspect<'s, S> for $marker<T> {
            fn fmt(value: &T, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                value.fmt(f)
            }

            fn eq(left: &T, right: &T) -> bool {
                left == right
            }
        }

        impl<'s, S: OwnedStorage<Error = Infallible>, T: Clone + 's> Duplicate<'s, S>
            for $marker<T>
        {
            fn duplicate(value: &T) -> T {
                value.clone()
            }
        }
    )*};
}

leaf!(Plain, Model);

#[cfg(test)]
mod tests;
