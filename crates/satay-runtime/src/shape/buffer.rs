//! Buffers self-describing input so union variants can be attempted in turn.
//!
//! A union cannot know its variant before reading the value, and each variant is
//! decoded into a storage context. Input is therefore captured once and replayed
//! by reference for every candidate; nothing reaches the storage policy until a
//! variant decodes it.

use std::fmt;
use std::marker::PhantomData;

use satay_storage::Storage;
use serde::de::value::{MapAccessDeserializer, MapDeserializer, SeqDeserializer};
use serde::de::{
    self, Deserialize, Deserializer, IntoDeserializer, MapAccess, SeqAccess, Unexpected,
};
use serde::forward_to_deserialize_any;

use super::Decode;

/// A captured value replayed while selecting a union variant.
pub struct Buffered<E> {
    node: Node,
    error: PhantomData<fn() -> E>,
}

impl<E> fmt::Debug for Buffered<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Buffered").field(&self.node).finish()
    }
}

impl<E: de::Error> Buffered<E> {
    /// Captures a whole value whose variant is found by trying each candidate.
    ///
    /// # Errors
    /// Returns the deserializer's error for malformed input.
    pub fn untagged<'de, D: Deserializer<'de, Error = E>>(deserializer: D) -> Result<Self, E> {
        Ok(Self {
            node: Node::deserialize(deserializer)?,
            error: PhantomData,
        })
    }

    /// Captures an object and removes its `tag` entry, returning the tag value.
    ///
    /// # Errors
    /// Returns an error if the input is not an object, or `tag` is missing or
    /// not a string.
    pub fn tagged<'de, D: Deserializer<'de, Error = E>>(
        deserializer: D,
        tag: &'static str,
    ) -> Result<(String, Self), E> {
        let Node::Map(mut entries) = Node::deserialize(deserializer)? else {
            return Err(E::custom(format_args!(
                "expected an object tagged by `{tag}`"
            )));
        };
        let position = entries
            .iter()
            .position(|(key, _)| matches!(key, Node::String(key) if key == tag))
            .ok_or_else(|| E::missing_field(tag))?;
        let value = match entries.remove(position).1 {
            Node::String(value) => value,
            other => return Err(E::invalid_type(other.unexpected(), &"a variant name")),
        };
        Ok((
            value,
            Self {
                node: Node::Map(entries),
                error: PhantomData,
            },
        ))
    }

    /// Decodes the captured value as shape `Sh` into `storage`.
    ///
    /// # Errors
    /// Returns an error if the captured value does not match the shape.
    pub fn decode<'c, 's, Sh, S>(&'c self, storage: &'s S) -> Result<Sh::Value, E>
    where
        Sh: Decode<'c, 's, S>,
        S: Storage<Error: fmt::Display> + 's,
    {
        Sh::decode(NodeDeserializer::new(&self.node), storage)
    }

    /// The error reported when a tagged union names no known variant.
    pub fn unknown_variant(&self, tag: &str, variants: &'static [&'static str]) -> E {
        E::unknown_variant(tag, variants)
    }

    /// The error reported when no variant of union `name` accepts the value.
    pub fn mismatch(&self, name: &'static str) -> E {
        E::custom(format_args!(
            "data did not match any variant of untagged enum {name}"
        ))
    }
}

#[derive(Debug)]
enum Node {
    Unit,
    Bool(bool),
    U64(u64),
    I64(i64),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>),
}

impl Node {
    fn unexpected(&self) -> Unexpected<'_> {
        match self {
            Self::Unit => Unexpected::Unit,
            Self::Bool(value) => Unexpected::Bool(*value),
            Self::U64(value) => Unexpected::Unsigned(*value),
            Self::I64(value) => Unexpected::Signed(*value),
            Self::F64(value) => Unexpected::Float(*value),
            Self::String(value) => Unexpected::Str(value),
            Self::Bytes(value) => Unexpected::Bytes(value),
            Self::Seq(_) => Unexpected::Seq,
            Self::Map(_) => Unexpected::Map,
        }
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Node;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("any value")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Node, E> {
                Ok(Node::Bool(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Node, E> {
                Ok(Node::I64(value))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Node, E> {
                Ok(Node::U64(value))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Node, E> {
                Ok(Node::F64(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Node, E> {
                Ok(Node::String(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Node, E> {
                Ok(Node::String(value))
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<Node, E> {
                Ok(Node::Bytes(value.to_vec()))
            }

            fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Node, E> {
                Ok(Node::Bytes(value))
            }

            fn visit_unit<E>(self) -> Result<Node, E> {
                Ok(Node::Unit)
            }

            fn visit_none<E>(self) -> Result<Node, E> {
                Ok(Node::Unit)
            }

            fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Node, D::Error> {
                Node::deserialize(deserializer)
            }

            fn visit_newtype_struct<D: Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Node, D::Error> {
                Node::deserialize(deserializer)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
                let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(1024));
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(Node::Seq(items))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
                let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0).min(1024));
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Node::Map(entries))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

struct NodeDeserializer<'c, E> {
    node: &'c Node,
    error: PhantomData<fn() -> E>,
}

impl<'c, E> NodeDeserializer<'c, E> {
    fn new(node: &'c Node) -> Self {
        Self {
            node,
            error: PhantomData,
        }
    }
}

impl<'c, E: de::Error> IntoDeserializer<'c, E> for &'c Node {
    type Deserializer = NodeDeserializer<'c, E>;

    fn into_deserializer(self) -> Self::Deserializer {
        NodeDeserializer::new(self)
    }
}

impl<'de, E: de::Error> Deserializer<'de> for NodeDeserializer<'de, E> {
    type Error = E;

    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, E> {
        match self.node {
            Node::Unit => visitor.visit_unit(),
            Node::Bool(value) => visitor.visit_bool(*value),
            Node::U64(value) => visitor.visit_u64(*value),
            Node::I64(value) => visitor.visit_i64(*value),
            Node::F64(value) => visitor.visit_f64(*value),
            Node::String(value) => visitor.visit_borrowed_str(value),
            Node::Bytes(value) => visitor.visit_borrowed_bytes(value),
            Node::Seq(items) => {
                let mut seq = SeqDeserializer::new(items.iter());
                let value = visitor.visit_seq(&mut seq)?;
                seq.end()?;
                Ok(value)
            }
            Node::Map(entries) => {
                let mut map = MapDeserializer::new(entries.iter().map(|(key, value)| (key, value)));
                let value = visitor.visit_map(&mut map)?;
                map.end()?;
                Ok(value)
            }
        }
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, E> {
        match self.node {
            Node::Unit => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, E> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, E> {
        match self.node {
            Node::String(variant) => visitor.visit_enum(variant.as_str().into_deserializer()),
            Node::Map(entries) if entries.len() == 1 => {
                visitor.visit_enum(MapAccessDeserializer::new(MapDeserializer::new(
                    entries.iter().map(|(key, value)| (key, value)),
                )))
            }
            other => Err(E::invalid_type(other.unexpected(), &"an enum")),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
        byte_buf unit unit_struct seq tuple tuple_struct map struct identifier
        ignored_any
    }
}
