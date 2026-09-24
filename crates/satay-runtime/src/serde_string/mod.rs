use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use serde::de::Error as DeError;

macro_rules! string_from_str_module {
    ($module:ident, $ty:ty) => {
        pub mod $module {
            pub fn serialize<S>(value: &$ty, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                super::serialize_display(value, serializer)
            }

            pub fn deserialize<'de, D>(deserializer: D) -> Result<$ty, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                super::deserialize_from_str(deserializer)
            }

            pub fn serialize_none_if<S>(
                value: &Option<$ty>,
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
                    serialize,
                )
            }

            pub fn deserialize_none_if<'de, D>(
                deserializer: D,
                none_if: &[&str],
            ) -> Result<Option<$ty>, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                super::deserialize_none_if_with(deserializer, none_if, str::parse::<$ty>)
            }

            pub mod option {
                pub fn serialize<S>(value: &Option<$ty>, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    super::super::serialize_option_display(value.as_ref(), serializer)
                }

                pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<$ty>, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    super::super::deserialize_option_from_str(deserializer)
                }

                pub fn deserialize_none_if<'de, D>(
                    deserializer: D,
                    none_if: &[&str],
                ) -> Result<Option<$ty>, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    super::super::deserialize_option_none_if_with(
                        deserializer,
                        none_if,
                        str::parse::<$ty>,
                    )
                }
            }
        }
    };
}

macro_rules! string_float_module {
    ($module:ident, $ty:ty) => {
        pub mod $module {
            use serde::Deserialize;
            use serde::de::Error as DeError;

            pub fn serialize<S>(value: &$ty, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                super::serialize_display(value, serializer)
            }

            pub fn deserialize<'de, D>(deserializer: D) -> Result<$ty, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = <String as Deserialize>::deserialize(deserializer)?;
                fast_float2::parse::<$ty, _>(&value).map_err(DeError::custom)
            }

            pub fn serialize_none_if<S>(
                value: &Option<$ty>,
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
                    serialize,
                )
            }

            pub fn deserialize_none_if<'de, D>(
                deserializer: D,
                none_if: &[&str],
            ) -> Result<Option<$ty>, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                super::deserialize_none_if_with(deserializer, none_if, |value| {
                    fast_float2::parse::<$ty, _>(value)
                })
            }

            pub mod option {
                use serde::Deserialize;
                use serde::de::Error as DeError;

                pub fn serialize<S>(value: &Option<$ty>, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    super::super::serialize_option_display(value.as_ref(), serializer)
                }

                pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<$ty>, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    let value = <Option<String> as Deserialize>::deserialize(deserializer)?;
                    value
                        .map(|value| fast_float2::parse::<$ty, _>(&value).map_err(DeError::custom))
                        .transpose()
                }

                pub fn deserialize_none_if<'de, D>(
                    deserializer: D,
                    none_if: &[&str],
                ) -> Result<Option<$ty>, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    super::super::deserialize_option_none_if_with(deserializer, none_if, |value| {
                        fast_float2::parse::<$ty, _>(value)
                    })
                }
            }
        }
    };
}

string_from_str_module!(as_url, crate::Url);
string_from_str_module!(as_u8, u8);
string_from_str_module!(as_u16, u16);
string_from_str_module!(as_u32, u32);
string_from_str_module!(as_u64, u64);
string_from_str_module!(as_i8, i8);
string_from_str_module!(as_i16, i16);
string_from_str_module!(as_i32, i32);
string_from_str_module!(as_i64, i64);
string_float_module!(as_f32, f32);
string_float_module!(as_f64, f64);
pub mod as_bool;
pub mod as_date;
pub mod as_naive_datetime;
pub mod as_offset_datetime;
pub mod as_time;
pub mod as_unix_time;
pub mod pair;

fn serialize_display<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: fmt::Display,
    S: serde::Serializer,
{
    serializer.serialize_str(&value.to_string())
}

fn serialize_option_display<T, S>(value: Option<&T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: fmt::Display,
    S: serde::Serializer,
{
    match value {
        Some(value) => serialize_display(value, serializer),
        None => serializer.serialize_none(),
    }
}

fn serialize_option_or_sentinel<T, S>(
    value: Option<&T>,
    none_value: &str,
    serializer: S,
    serialize: impl FnOnce(&T, S) -> Result<S::Ok, S::Error>,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(value) => serialize(value, serializer),
        None => serializer.serialize_str(none_value),
    }
}

fn deserialize_from_str<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr,
    T::Err: fmt::Display,
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    value.parse::<T>().map_err(DeError::custom)
}

fn deserialize_option_from_str<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: FromStr,
    T::Err: fmt::Display,
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| value.parse::<T>().map_err(DeError::custom))
        .transpose()
}

fn deserialize_none_if_with<'de, T, D, E>(
    deserializer: D,
    none_if: &[&str],
    parse: impl FnOnce(&str) -> Result<T, E>,
) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    E: fmt::Display,
{
    let value = String::deserialize(deserializer)?;
    parse_none_if_with(&value, none_if, parse)
}

fn deserialize_option_none_if_with<'de, T, D, E>(
    deserializer: D,
    none_if: &[&str],
    parse: impl FnOnce(&str) -> Result<T, E>,
) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    E: fmt::Display,
{
    let Some(value) = Option::<String>::deserialize(deserializer)? else {
        return Ok(None);
    };
    parse_none_if_with(&value, none_if, parse)
}

fn parse_none_if_with<T, E, D>(
    value: &str,
    none_if: &[&str],
    parse: impl FnOnce(&str) -> Result<T, E>,
) -> Result<Option<T>, D>
where
    E: fmt::Display,
    D: DeError,
{
    if none_if.contains(&value) {
        return Ok(None);
    }
    parse(value).map(Some).map_err(DeError::custom)
}

fn deserialize_bool(value: &str) -> Result<bool, &'static str> {
    match value {
        "1" => Ok(true),
        "0" => Ok(false),
        value if value.eq_ignore_ascii_case("true") => Ok(true),
        value if value.eq_ignore_ascii_case("false") => Ok(false),
        _ => Err("invalid boolean string"),
    }
}
