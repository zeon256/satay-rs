/// Field-local string codecs for a pair of values.
///
/// The generated model supplies its constructor so the target type keeps its
/// ordinary object representation and its existing numeric validations.
use std::fmt;

use serde::{
    de::{Error as DeError, Visitor},
    ser::Error,
};

pub fn serialize<A, B, S>(
    first: &A,
    second: &B,
    delimiter: &str,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    A: fmt::Display,
    B: fmt::Display,
    S: serde::Serializer,
{
    if delimiter.is_empty() {
        return Err(Error::custom("pair delimiter must not be empty"));
    }
    serializer.collect_str(&format_args!("{first}{delimiter}{second}"))
}

pub fn deserialize<'de, T, D>(
    deserializer: D,
    delimiter: &str,
    parse: impl FnOnce(&str, &str) -> Result<T, D::Error>,
) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(StringVisitor(|value: &str| {
        parse_with(value, delimiter, parse)
    }))
}

pub fn deserialize_none_if<'de, T, D>(
    deserializer: D,
    delimiter: &str,
    none_if: &[&str],
    parse: impl FnOnce(&str, &str) -> Result<T, D::Error>,
) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(StringVisitor(|value: &str| -> Result<_, D::Error> {
        super::parse_none_if_with(value, none_if, |value| parse_with(value, delimiter, parse))
    }))
}

pub mod option {
    use crate::serde_string::parse_none_if_with;

    use super::{OptionVisitor, parse_with};

    pub fn deserialize<'de, T, D>(
        deserializer: D,
        delimiter: &str,
        parse: impl FnOnce(&str, &str) -> Result<T, D::Error>,
    ) -> Result<Option<T>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_none_if(deserializer, delimiter, &[], parse)
    }

    pub fn deserialize_none_if<'de, T, D>(
        deserializer: D,
        delimiter: &str,
        none_if: &[&str],
        parse: impl FnOnce(&str, &str) -> Result<T, D::Error>,
    ) -> Result<Option<T>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_option(OptionVisitor(|value: &str| -> Result<_, D::Error> {
            parse_none_if_with(value, none_if, |value| parse_with(value, delimiter, parse))
        }))
    }

    /// Consumes the complete JSON value before discarding invalid input.
    #[cfg(feature = "json")]
    pub fn deserialize_lossy<'de, T, D>(
        deserializer: D,
        delimiter: &str,
        parse: impl FnOnce(&str, &str) -> Result<T, D::Error>,
    ) -> Result<Option<T>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::Deserialize;
        use serde_json::Value;

        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            Value::String(value) => Ok(parse_with(&value, delimiter, parse).ok()),
            _ => Ok(None),
        }
    }
}

fn parse_with<T, E>(
    value: &str,
    delimiter: &str,
    parse: impl FnOnce(&str, &str) -> Result<T, E>,
) -> Result<T, E>
where
    E: DeError,
{
    if delimiter.is_empty() {
        return Err(E::custom("pair delimiter must not be empty"));
    }
    let mut components = value.trim().split(delimiter);
    let first = components.next().unwrap_or_default().trim();
    let second = components.next().unwrap_or_default().trim();
    if first.is_empty() || second.is_empty() || components.next().is_some() {
        return Err(E::custom("expected exactly two nonempty pair components"));
    }
    parse(first, second)
}

struct StringVisitor<F>(F);

impl<T, E, F> Visitor<'_> for StringVisitor<F>
where
    E: fmt::Display,
    F: FnOnce(&str) -> Result<T, E>,
{
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a string containing two delimited components")
    }

    fn visit_str<Error>(self, value: &str) -> Result<T, Error>
    where
        Error: DeError,
    {
        (self.0)(value).map_err(Error::custom)
    }
}

struct OptionVisitor<F>(F);

impl<'de, T, E, F> Visitor<'de> for OptionVisitor<F>
where
    E: fmt::Display,
    F: FnOnce(&str) -> Result<Option<T>, E>,
{
    type Value = Option<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("null or a string containing two delimited components")
    }

    fn visit_none<Error>(self) -> Result<Self::Value, Error>
    where
        Error: DeError,
    {
        Ok(None)
    }

    fn visit_unit<Error>(self) -> Result<Self::Value, Error>
    where
        Error: DeError,
    {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(StringVisitor(self.0))
    }
}
