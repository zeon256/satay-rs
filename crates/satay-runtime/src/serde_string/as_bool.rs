use std::fmt;

use serde::de::{Error as DeError, Visitor};

pub fn serialize<S>(value: &bool, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(crate::format_bool(value))
}
pub fn serialize_mapped<S>(
    value: &bool,
    true_value: &str,
    false_value: &str,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(if *value { true_value } else { false_value })
}
pub fn serialize_mapped_none_if<S>(
    value: &Option<bool>,
    true_value: &str,
    false_value: &str,
    none_value: &str,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    super::serialize_option_or_sentinel(
        value.as_ref(),
        none_value,
        serializer,
        |value, serializer| serialize_mapped(value, true_value, false_value, serializer),
    )
}
pub fn deserialize<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_any(BoolVisitor)
}
pub fn deserialize_mapped<'de, D>(
    deserializer: D,
    true_values: &[&str],
    false_values: &[&str],
    unknown_as: Option<bool>,
) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_any(MappedBoolVisitor {
        true_values,
        false_values,
        unknown_as,
    })
}
pub fn serialize_none_if<S>(
    value: &Option<bool>,
    none_value: &str,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    super::serialize_option_or_sentinel(value.as_ref(), none_value, serializer, serialize)
}
pub fn deserialize_none_if<'de, D>(
    deserializer: D,
    none_if: &[&str],
) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_any(BoolNoneIfVisitor { none_if })
}
pub fn deserialize_mapped_none_if<'de, D>(
    deserializer: D,
    true_values: &[&str],
    false_values: &[&str],
    unknown_as: Option<bool>,
    none_if: &[&str],
) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_any(MappedBoolNoneIfVisitor {
        true_values,
        false_values,
        unknown_as,
        none_if,
    })
}
struct BoolVisitor;
impl Visitor<'_> for BoolVisitor {
    type Value = bool;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a boolean string or numeric boolean")
    }
    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(value)
    }
    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        super::deserialize_bool(value).map_err(DeError::custom)
    }
    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        self.visit_str(&value)
    }
    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DeError::custom("invalid boolean number")),
        }
    }
    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DeError::custom("invalid boolean number")),
        }
    }
}
struct MappedBoolVisitor<'a> {
    true_values: &'a [&'a str],
    false_values: &'a [&'a str],
    unknown_as: Option<bool>,
}
impl Visitor<'_> for MappedBoolVisitor<'_> {
    type Value = bool;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a configured boolean string or numeric boolean")
    }
    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(value)
    }
    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        mapped_bool(value, self.true_values, self.false_values, self.unknown_as)
            .ok_or_else(|| DeError::custom("invalid mapped boolean string"))
    }
    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        self.visit_str(&value)
    }
    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DeError::custom("invalid boolean number")),
        }
    }
    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DeError::custom("invalid boolean number")),
        }
    }
}
struct BoolNoneIfVisitor<'a> {
    none_if: &'a [&'a str],
}
impl Visitor<'_> for BoolNoneIfVisitor<'_> {
    type Value = Option<bool>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a boolean string, numeric boolean, or configured sentinel")
    }
    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Some(value))
    }
    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        if self.none_if.contains(&value) {
            Ok(None)
        } else {
            super::deserialize_bool(value)
                .map(Some)
                .map_err(DeError::custom)
        }
    }
    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        self.visit_str(&value)
    }
    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        match value {
            0 => Ok(Some(false)),
            1 => Ok(Some(true)),
            _ => Err(DeError::custom("invalid boolean number")),
        }
    }
    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        match value {
            0 => Ok(Some(false)),
            1 => Ok(Some(true)),
            _ => Err(DeError::custom("invalid boolean number")),
        }
    }
}
struct MappedBoolNoneIfVisitor<'a> {
    true_values: &'a [&'a str],
    false_values: &'a [&'a str],
    unknown_as: Option<bool>,
    none_if: &'a [&'a str],
}
impl Visitor<'_> for MappedBoolNoneIfVisitor<'_> {
    type Value = Option<bool>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a configured boolean string, numeric boolean, or configured sentinel")
    }
    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Some(value))
    }
    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        if self.none_if.contains(&value) {
            Ok(None)
        } else {
            mapped_bool(value, self.true_values, self.false_values, self.unknown_as)
                .map(Some)
                .ok_or_else(|| DeError::custom("invalid mapped boolean string"))
        }
    }
    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        self.visit_str(&value)
    }
    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        match value {
            0 => Ok(Some(false)),
            1 => Ok(Some(true)),
            _ => Err(DeError::custom("invalid boolean number")),
        }
    }
    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        match value {
            0 => Ok(Some(false)),
            1 => Ok(Some(true)),
            _ => Err(DeError::custom("invalid boolean number")),
        }
    }
}
pub mod option {
    use std::fmt;

    use serde::de::Visitor;
    pub fn serialize<S>(value: &Option<bool>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match value {
            Some(value) => super::serialize(value, serializer),
            None => serializer.serialize_none(),
        }
    }
    pub fn serialize_mapped<S>(
        value: &Option<bool>,
        true_value: &str,
        false_value: &str,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match value {
            Some(value) => super::serialize_mapped(value, true_value, false_value, serializer),
            None => serializer.serialize_none(),
        }
    }
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_option(BoolOptionVisitor)
    }
    pub fn deserialize_mapped<'de, D>(
        deserializer: D,
        true_values: &[&str],
        false_values: &[&str],
        unknown_as: Option<bool>,
    ) -> Result<Option<bool>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_option(MappedBoolOptionVisitor {
            true_values,
            false_values,
            unknown_as,
            none_if: None,
        })
    }
    pub fn deserialize_none_if<'de, D>(
        deserializer: D,
        none_if: &[&str],
    ) -> Result<Option<bool>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_option(BoolOptionNoneIfVisitor { none_if })
    }
    pub fn deserialize_mapped_none_if<'de, D>(
        deserializer: D,
        true_values: &[&str],
        false_values: &[&str],
        unknown_as: Option<bool>,
        none_if: &[&str],
    ) -> Result<Option<bool>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_option(MappedBoolOptionVisitor {
            true_values,
            false_values,
            unknown_as,
            none_if: Some(none_if),
        })
    }
    struct BoolOptionVisitor;
    impl<'de> Visitor<'de> for BoolOptionVisitor {
        type Value = Option<bool>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an optional boolean string or numeric boolean")
        }
        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            super::deserialize(deserializer).map(Some)
        }
    }
    struct BoolOptionNoneIfVisitor<'a> {
        none_if: &'a [&'a str],
    }
    impl<'de> Visitor<'de> for BoolOptionNoneIfVisitor<'_> {
        type Value = Option<bool>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an optional boolean string, numeric boolean, or sentinel")
        }
        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(super::BoolNoneIfVisitor {
                none_if: self.none_if,
            })
        }
    }
    struct MappedBoolOptionVisitor<'a> {
        true_values: &'a [&'a str],
        false_values: &'a [&'a str],
        unknown_as: Option<bool>,
        none_if: Option<&'a [&'a str]>,
    }
    impl<'de> Visitor<'de> for MappedBoolOptionVisitor<'_> {
        type Value = Option<bool>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an optional configured boolean string or numeric boolean")
        }
        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            match self.none_if {
                Some(none_if) => deserializer.deserialize_any(super::MappedBoolNoneIfVisitor {
                    true_values: self.true_values,
                    false_values: self.false_values,
                    unknown_as: self.unknown_as,
                    none_if,
                }),
                None => super::deserialize_mapped(
                    deserializer,
                    self.true_values,
                    self.false_values,
                    self.unknown_as,
                )
                .map(Some),
            }
        }
    }
}
fn mapped_bool(
    value: &str,
    true_values: &[&str],
    false_values: &[&str],
    unknown_as: Option<bool>,
) -> Option<bool> {
    if true_values.contains(&value) {
        Some(true)
    } else if false_values.contains(&value) {
        Some(false)
    } else {
        unknown_as
    }
}
