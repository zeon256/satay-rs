use serde::Deserialize;
use serde::de::Error as DeError;
use serde::ser::Error as SerError;

use crate::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub fn serialize<S>(value: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let value = value.format(&Rfc3339).map_err(SerError::custom)?;
    serializer.serialize_str(&value)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <String as Deserialize>::deserialize(deserializer)?;
    OffsetDateTime::parse(&value, &Rfc3339).map_err(DeError::custom)
}

pub fn serialize_none_if<S>(
    value: &Option<OffsetDateTime>,
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
) -> Result<Option<OffsetDateTime>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    super::deserialize_none_if_with(deserializer, none_if, |value| {
        OffsetDateTime::parse(value, &Rfc3339)
    })
}

pub mod option {
    use serde::Deserialize;
    use serde::de::Error as DeError;

    use super::super as serde_string;
    use crate::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    pub fn serialize<S>(value: &Option<OffsetDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match value {
            Some(value) => super::serialize(value, serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<OffsetDateTime>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <Option<String> as Deserialize>::deserialize(deserializer)?;
        value
            .map(|value| OffsetDateTime::parse(&value, &Rfc3339).map_err(DeError::custom))
            .transpose()
    }

    pub fn deserialize_none_if<'de, D>(
        deserializer: D,
        none_if: &[&str],
    ) -> Result<Option<OffsetDateTime>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde_string::deserialize_option_none_if_with(deserializer, none_if, |value| {
            OffsetDateTime::parse(value, &Rfc3339)
        })
    }
}
