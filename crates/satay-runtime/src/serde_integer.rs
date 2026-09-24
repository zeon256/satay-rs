pub mod as_unix_time {
    use serde::Deserialize;
    use serde::de::Error as DeError;

    use crate::OffsetDateTime;

    pub fn serialize<S>(value: &OffsetDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i64(value.unix_timestamp())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <i64 as Deserialize>::deserialize(deserializer)?;
        OffsetDateTime::from_unix_timestamp(value).map_err(DeError::custom)
    }

    pub mod option {
        use serde::Deserialize;
        use serde::de::Error as DeError;

        use crate::OffsetDateTime;

        pub fn serialize<S>(
            value: &Option<OffsetDateTime>,
            serializer: S,
        ) -> Result<S::Ok, S::Error>
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
            let value = <Option<i64> as Deserialize>::deserialize(deserializer)?;
            value
                .map(|value| OffsetDateTime::from_unix_timestamp(value).map_err(DeError::custom))
                .transpose()
        }
    }
}

pub mod as_bool {
    use crate::serde_string::as_bool as string_bool;

    pub fn serialize<S>(value: &bool, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(u8::from(*value))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<bool, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        string_bool::deserialize(deserializer)
    }

    pub mod option {
        use crate::serde_string::as_bool::option as string_bool_option;

        pub fn serialize<S>(value: &Option<bool>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            match value {
                Some(value) => super::serialize(value, serializer),
                None => serializer.serialize_none(),
            }
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            string_bool_option::deserialize(deserializer)
        }
    }
}
