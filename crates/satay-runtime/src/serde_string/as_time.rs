use serde::Deserialize;
use serde::de::Error as DeError;

use crate::Time;

pub fn serialize<S>(value: &Time, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&crate::format_time(value))
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Time, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <String as Deserialize>::deserialize(deserializer)?;
    crate::parse_time(&value).map_err(DeError::custom)
}

pub fn serialize_none_if<S>(
    value: &Option<Time>,
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
) -> Result<Option<Time>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    super::deserialize_none_if_with(deserializer, none_if, crate::parse_time)
}

pub mod option {
    use serde::Deserialize;
    use serde::de::Error as DeError;

    use crate::Time;

    pub fn serialize<S>(value: &Option<Time>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match value {
            Some(value) => super::serialize(value, serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Time>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <Option<String> as Deserialize>::deserialize(deserializer)?;
        let Some(value) = value else {
            return Ok(None);
        };
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        crate::parse_time(value).map(Some).map_err(DeError::custom)
    }

    pub fn deserialize_none_if<'de, D>(
        deserializer: D,
        none_if: &[&str],
    ) -> Result<Option<Time>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <Option<String> as Deserialize>::deserialize(deserializer)?;
        let Some(value) = value else {
            return Ok(None);
        };
        if none_if.contains(&value.as_str()) {
            return Ok(None);
        }
        let value = value.trim();
        if value.is_empty() {
            return Ok(None);
        }
        crate::parse_time(value).map(Some).map_err(DeError::custom)
    }
}
