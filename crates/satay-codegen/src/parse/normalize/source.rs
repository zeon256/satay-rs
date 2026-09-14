//! Sparse source-presence index for the OpenAPI-to-IR frontend.
//!
//! Production `oas3` typed fields collapse `default: null` into "no default"
//! and default-valued `servers`/`security` vectors into "absent". Production
//! serde behavior must not change, so this frontend reads the same input
//! through [`serde_saphyr`] a second time and records only which objects
//! carry those keys. Only retained pointers are allocated; scalar values are
//! consumed and discarded.

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_saphyr::options;

use crate::error::ParseError;

use satay_ir::SourceRef;

/// Object keys whose declared presence survives typed parsing only through
/// this index.
const PRESENCE_KEYS: [&str; 3] = ["default", "servers", "security"];

/// Sparse set of RFC 6901 pointers pointing at retained presence keys.
#[derive(Debug, Default)]
pub(in crate::parse) struct PresenceIndex {
    present: BTreeSet<String>,
}

impl PresenceIndex {
    /// Reads the presence index from the same input the typed parser uses.
    ///
    /// JSON and YAML are handled by the same reader and options, matching
    /// [`oas3::from_yaml`]. YAML anchors and aliases resolve to the same
    /// values the typed parse sees, so aliased declarations are indexed at
    /// their alias location.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`serde_saphyr`] error, wrapped as
    /// [`ParseError::OpenApiDocument`], preserving the structured cause.
    pub(in crate::parse) fn read(spec: &str) -> Result<Self, ParseError> {
        serde_saphyr::from_str_with_options(spec, options! { strict_booleans: true })
            .map_err(ParseError::from)
    }

    /// Returns whether `object_pointer` declares `keyword`.
    pub(in crate::parse) fn has(&self, object_pointer: &str, keyword: &str) -> bool {
        self.present
            .contains(&child_pointer(object_pointer, keyword))
    }
}

/// Builds the child pointer `parent` + one escaped `token`.
pub(in crate::parse) fn child_pointer(parent: &str, token: &str) -> String {
    let mut pointer = String::with_capacity(parent.len() + token.len() + 1);
    pointer.push_str(parent);
    pointer.push('/');
    escape_token_into(&mut pointer, token);
    pointer
}

fn escape_token_into(pointer: &mut String, token: &str) {
    for character in token.chars() {
        match character {
            '~' => pointer.push_str("~0"),
            '/' => pointer.push_str("~1"),
            _ => pointer.push(character),
        }
    }
}

/// Builds provenance for one schema use or HTTP record.
pub(in crate::parse) fn source_ref(document_id: &str, pointer: &str) -> SourceRef {
    SourceRef {
        document: document_id.to_owned(),
        pointer: pointer.to_owned(),
    }
}

impl<'de> serde::Deserialize<'de> for PresenceIndex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut pointer = String::new();
        let mut present = BTreeSet::new();
        deserializer.deserialize_map(PresenceSeed {
            pointer: &mut pointer,
            present: &mut present,
        })?;
        Ok(Self { present })
    }
}

/// Collects `default`, `servers`, and `security` key pointers while walking
/// the document. Scalar values are discarded; map and sequence entries are
/// always consumed so nested component declarations are indexed.
struct PresenceSeed<'a> {
    pointer: &'a mut String,
    present: &'a mut BTreeSet<String>,
}

impl<'de> DeserializeSeed<'de> for PresenceSeed<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for PresenceSeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any OpenAPI value")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let PresenceSeed { pointer, present } = self;
        let mut index = 0usize;
        loop {
            let saved = pointer.len();
            pointer.push('/');
            pointer.push_str(&index.to_string());
            let element = sequence.next_element_seed(PresenceSeed {
                pointer: &mut *pointer,
                present: &mut *present,
            })?;
            pointer.truncate(saved);
            if element.is_none() {
                return Ok(());
            }
            index += 1;
        }
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let PresenceSeed { pointer, present } = self;
        while let Some(key) = map.next_key::<String>()? {
            let saved = pointer.len();
            pointer.push('/');
            escape_token_into(pointer, &key);
            if PRESENCE_KEYS.contains(&key.as_str()) {
                present.insert(pointer.clone());
            }
            map.next_value_seed(PresenceSeed {
                pointer: &mut *pointer,
                present: &mut *present,
            })?;
            pointer.truncate(saved);
        }
        Ok(())
    }
}
