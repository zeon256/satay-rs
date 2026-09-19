use crate::serde_string::as_f64;

#[allow(clippy::ref_option)]
pub fn serialize<S>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    as_f64::serialize_none_if(value, "NA", serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    as_f64::deserialize_none_if(deserializer, &["NA", "-"])
}
