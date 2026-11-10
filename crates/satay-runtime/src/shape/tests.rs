#![allow(
    clippy::mutable_key_type,
    reason = "bumpalo strings are compared by content; their allocator handle is not part of the key"
)]

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt;

use bumpalo::Bump;
use bumpalo::collections::{String as BumpString, Vec as BumpVec};
use satay_storage::{AllocStorage, BoxedStorage, OwnedStorage, Storage};

use super::{Contiguous, Map, Optional, Plain, Shape, Text};

#[derive(Default)]
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
        entries: impl IntoIterator<Item = (Self::Text<'a>, V)>,
    ) -> Result<Self::Map<'a, V>, Self::Error> {
        Ok(entries.into_iter().collect())
    }
}

/// Owned policy that rejects text longer than three bytes and more than two elements.
struct Tight;

#[derive(Debug)]
struct Full(&'static str);

impl fmt::Display for Full {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} capacity exceeded", self.0)
    }
}

impl Storage for Tight {
    type Error = Full;
    type Text<'a> = String;
    type Contiguous<'a, T: 'a> = Vec<T>;
    type Map<'a, V: 'a> = BTreeMap<String, V>;

    fn try_text(&self, value: &str) -> Result<String, Full> {
        if value.len() > 3 {
            return Err(Full("text"));
        }
        Ok(value.to_owned())
    }

    fn try_contiguous<'a, T: 'a>(
        &'a self,
        values: impl IntoIterator<Item = T>,
    ) -> Result<Vec<T>, Full> {
        let mut stored = vec![];
        for value in values {
            if stored.len() == 2 {
                return Err(Full("collection"));
            }
            stored.push(value);
        }
        Ok(stored)
    }

    fn try_map<'a, V: 'a>(
        &'a self,
        entries: impl IntoIterator<Item = (String, V)>,
    ) -> Result<BTreeMap<String, V>, Full> {
        Ok(entries.into_iter().collect())
    }
}

type Nested = Map<Contiguous<Optional<Text>>>;
type NestedValue<'s, S> = <Nested as Shape<'s, S>>::Value;

const NESTED: &[u8] = br#"{"b":["x",null],"a":[]}"#;

fn decode<'s, S: Storage<Error: fmt::Display>>(body: &[u8], storage: &'s S) -> NestedValue<'s, S> {
    super::from_json_slice::<Nested, S>(body, storage).unwrap()
}

fn encode<'s, S: Storage + 's>(value: &NestedValue<'s, S>) -> String {
    serde_json::to_string(&super::view::<Nested, S>(value)).unwrap()
}

#[test]
fn default_policy_shapes_are_plain_std_collections() {
    let value: BTreeMap<String, Vec<Option<String>>> = decode(NESTED, &AllocStorage);
    assert_eq!(value["b"], [Some("x".to_owned()), None]);
    assert!(value["a"].is_empty());
}

#[test]
fn every_policy_round_trips_the_same_wire_form() {
    let wire = r#"{"a":[],"b":["x",null]}"#;
    let mut boxed = decode(NESTED, &BoxedStorage);
    assert_eq!(encode::<BoxedStorage>(&boxed), wire);
    let _: Box<[Option<Box<str>>]> = boxed.remove("b").unwrap();
    assert_eq!(encode::<AllocStorage>(&decode(NESTED, &AllocStorage)), wire);

    let arena = Arena::default();
    let stored = decode(NESTED, &arena);
    let _: &BumpVec<'_, Option<BumpString<'_>>> = &stored["b"];
    assert_eq!(encode::<Arena>(&stored), wire);
    assert!(arena.0.allocated_bytes() > 0);
}

#[test]
fn inspection_matches_std_formatting_and_ignores_map_order() {
    let arena = Arena::default();
    let stored = decode(NESTED, &arena);
    let owned = decode(NESTED, &AllocStorage);
    assert_eq!(
        format!("{:?}", super::view::<Nested, Arena>(&stored)),
        format!("{owned:?}")
    );

    let reordered = decode(br#"{"a":[],"b":["x",null]}"#, &arena);
    assert!(super::eq::<Nested, Arena>(&stored, &reordered));
    for different in [
        &br#"{"a":[],"b":["x","y"]}"#[..],
        br#"{"a":[],"b":["x"]}"#,
        br#"{"a":[],"c":["x",null]}"#,
        br#"{"b":["x",null]}"#,
    ] {
        assert!(!super::eq::<Nested, Arena>(
            &stored,
            &decode(different, &arena)
        ));
    }
}

#[test]
fn duplication_rebuilds_owned_values() {
    let boxed = decode(NESTED, &BoxedStorage);
    let copy = super::duplicate::<Nested, BoxedStorage>(&boxed);
    assert_eq!(copy, boxed);
    assert_eq!(&*super::owned_text::<BoxedStorage>("cat"), "cat");
}

#[test]
fn element_errors_are_reported_instead_of_a_truncated_collection() {
    let error = super::from_json_slice::<Contiguous<Plain<u8>>, AllocStorage>(
        b"[1,2,\"three\",4]",
        &AllocStorage,
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid type"), "{error}");

    let error =
        super::from_json_slice::<Map<Plain<u8>>, AllocStorage>(br#"{"a":1,"b":[]}"#, &AllocStorage)
            .unwrap_err();
    assert!(error.to_string().contains("invalid type"), "{error}");
}

#[test]
fn storage_failures_surface_as_decode_errors() {
    let short = super::from_json_slice::<Contiguous<Text>, Tight>(br#"["ab","cd"]"#, &Tight);
    assert_eq!(short.unwrap(), ["ab", "cd"]);

    let text = super::from_json_slice::<Contiguous<Text>, Tight>(br#"["abcd"]"#, &Tight);
    assert!(text.unwrap_err().to_string().contains("text capacity"));

    let collection = super::from_json_slice::<Contiguous<Plain<u8>>, Tight>(b"[1,2,3]", &Tight);
    assert!(
        collection
            .unwrap_err()
            .to_string()
            .contains("collection capacity")
    );
}

#[test]
fn trailing_input_is_rejected() {
    assert!(super::from_json_slice::<Text, AllocStorage>(br#""a" "b""#, &AllocStorage).is_err());
}

#[test]
fn lossy_values_become_none_only_for_shape_mismatches() {
    use serde::de::DeserializeSeed;

    let lossy = |input: &str| {
        let mut deserializer = serde_json::Deserializer::from_str(input);
        super::lossy::<Contiguous<Text>, AllocStorage>(&AllocStorage).deserialize(&mut deserializer)
    };
    assert_eq!(lossy(r#"["a"]"#).unwrap(), Some(vec!["a".to_owned()]));
    assert_eq!(lossy(r#"{"a":1}"#).unwrap(), None);
    assert_eq!(lossy("null").unwrap(), None);
    assert!(lossy(r#"["a""#).is_err());
}

#[test]
fn projected_bodies_decode_into_storage() {
    let arena = Arena::default();
    let names = super::from_projected_json_slice::<Contiguous<Optional<Text>>, Arena>(
        br#"{"value":[{"name":"a"},{}]}"#,
        "value",
        Some("name"),
        &arena,
    )
    .unwrap();
    assert_eq!(names[0].as_deref(), Some("a"));
    assert!(names[1].is_none());
}

fn parts<B>(body: B) -> crate::RequestParts<B> {
    crate::RequestParts {
        method: http::Method::POST,
        uri: "/pets".to_owned(),
        headers: http::HeaderMap::new(),
        body,
    }
}

#[test]
fn optional_json_bodies_are_omitted_when_absent() {
    let present = super::into_optional_json_request::<Contiguous<Text>, BoxedStorage>(parts(Some(
        BoxedStorage::CONTEXT
            .try_contiguous([Box::<str>::from("a")])
            .unwrap(),
    )))
    .unwrap();
    assert_eq!(present.body(), br#"["a"]"#);
    assert_eq!(present.headers()["content-type"], "application/json");

    let absent =
        super::into_optional_json_request::<Contiguous<Text>, BoxedStorage>(parts(None)).unwrap();
    assert!(absent.body().is_empty());
    assert!(absent.headers().is_empty());

    let required = super::into_json_request::<Text, AllocStorage>(parts("a".to_owned())).unwrap();
    assert_eq!(required.body(), br#""a""#);
}

#[test]
fn buffered_unions_replay_input_for_each_candidate() {
    let arena = Arena::default();
    let mut deserializer = serde_json::Deserializer::from_str(r#"{"n":[1,-2,3.5],"s":"x"}"#);
    let buffered = super::Buffered::untagged(&mut deserializer).unwrap();
    assert!(buffered.decode::<Contiguous<Text>, Arena>(&arena).is_err());
    assert!(buffered.decode::<Plain<u8>, Arena>(&arena).is_err());
    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Numbers {
        n: (u8, i8, f32),
        s: Kind,
    }
    #[derive(Debug, serde::Deserialize, PartialEq)]
    #[serde(rename_all = "lowercase")]
    enum Kind {
        X,
    }
    assert_eq!(
        buffered.decode::<Plain<Numbers>, Arena>(&arena).unwrap(),
        Numbers {
            n: (1, -2, 3.5),
            s: Kind::X
        }
    );
    assert_eq!(
        buffered.mismatch("Choice").to_string(),
        "data did not match any variant of untagged enum Choice"
    );
}

#[test]
fn tagged_unions_remove_the_tag_wherever_it_appears() {
    let tagged = |input: &str| {
        let mut deserializer = serde_json::Deserializer::from_str(input);
        super::Buffered::tagged(&mut deserializer, "type")
    };
    let (tag, content) = tagged(r#"{"name":"a","type":"cat","lives":9}"#).unwrap();
    assert_eq!(tag, "cat");
    let rest = content
        .decode::<Plain<BTreeMap<String, serde_json::Value>>, AllocStorage>(&AllocStorage)
        .unwrap();
    assert_eq!(rest.keys().collect::<Vec<_>>(), ["lives", "name"]);

    assert!(
        tagged(r#"{"name":"a"}"#)
            .unwrap_err()
            .to_string()
            .contains("missing field `type`")
    );
    assert!(tagged(r#"{"type":1}"#).is_err());
    assert!(tagged(r#"["type"]"#).is_err());
}

#[test]
fn field_seeds_index_known_keys_and_flag_unknown_ones() {
    use serde::de::{DeserializeSeed, IntoDeserializer, value};

    let index = |key: &'static str| {
        super::Field(&["id", "name"])
            .deserialize(IntoDeserializer::<value::Error>::into_deserializer(key))
            .unwrap()
    };
    assert_eq!((index("id"), index("name"), index("other")), (0, 1, 2));
}
