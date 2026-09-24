use serde::Deserialize;
use serde::de::Error as DeError;

use crate::Date;

pub fn serialize<S>(value: &Date, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&crate::format_date(value))
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Date, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <String as Deserialize>::deserialize(deserializer)?;
    crate::parse_date(&value).map_err(DeError::custom)
}

pub fn serialize_none_if<S>(
    value: &Option<Date>,
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
) -> Result<Option<Date>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    super::deserialize_none_if_with(deserializer, none_if, crate::parse_date)
}

pub mod option {
    use serde::Deserialize;
    use serde::de::Error as DeError;

    use super::super as serde_string;
    use crate::Date;

    pub fn serialize<S>(value: &Option<Date>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match value {
            Some(value) => super::serialize(value, serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Date>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <Option<String> as Deserialize>::deserialize(deserializer)?;
        value
            .map(|value| crate::parse_date(&value).map_err(DeError::custom))
            .transpose()
    }

    pub fn deserialize_none_if<'de, D>(
        deserializer: D,
        none_if: &[&str],
    ) -> Result<Option<Date>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde_string::deserialize_option_none_if_with(deserializer, none_if, crate::parse_date)
    }
}
