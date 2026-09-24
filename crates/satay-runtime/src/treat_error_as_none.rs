use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Serializes an optional value, omitting `None`.
///
/// # Errors
///
/// Returns an error if serializing the inner value fails.
pub fn serialize<S, T>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    match value {
        Some(inner) => inner.serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// Deserializes an optional value, treating invalid inner values as `None`.
///
/// # Errors
///
/// Returns an error if the input cannot be read as JSON.
pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match T::deserialize(value) {
        Ok(parsed) => Ok(Some(parsed)),
        Err(_) => Ok(None),
    }
}
