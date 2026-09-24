use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseRangeError {
    #[error("range contains more than one `-` separator")]
    TooManySeparators,

    #[error("invalid range minimum `{value}`: {message}")]
    InvalidMinimum { value: String, message: String },

    #[error("invalid range maximum `{value}`: {message}")]
    InvalidMaximum { value: String, message: String },
}
/// Parses an inclusive range string into optional minimum and maximum bounds.
///
/// # Errors
///
/// Returns an error if the range has too many separators or either bound cannot be parsed as `T`.
pub fn parse_range<T>(value: &str) -> Result<(Option<T>, Option<T>), ParseRangeError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    let value = value.trim();
    if value.is_empty() {
        return Ok((None, None));
    }

    let (min, max) = match value.split_once('-') {
        Some((min, max)) => {
            if max.contains('-') {
                return Err(ParseRangeError::TooManySeparators);
            }
            (min, max)
        }
        None => (value, value),
    };

    Ok((parse_range_min(min)?, parse_range_max(max)?))
}
#[must_use]
pub fn format_range<T>(min: &Option<T>, max: &Option<T>) -> String
where
    T: fmt::Display,
{
    match (min, max) {
        (Some(min), Some(max)) => format!("{min}-{max}"),
        (Some(min), None) => format!("{min}-"),
        (None, Some(max)) => format!("-{max}"),
        (None, None) => String::new(),
    }
}
fn parse_range_min<T>(value: &str) -> Result<Option<T>, ParseRangeError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    parse_range_bound(value, |value, message| ParseRangeError::InvalidMinimum {
        value,
        message,
    })
}
fn parse_range_max<T>(value: &str) -> Result<Option<T>, ParseRangeError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    parse_range_bound(value, |value, message| ParseRangeError::InvalidMaximum {
        value,
        message,
    })
}
fn parse_range_bound<T>(
    value: &str,
    invalid: impl FnOnce(String, String) -> ParseRangeError,
) -> Result<Option<T>, ParseRangeError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }

    value
        .parse::<T>()
        .map(Some)
        .map_err(|err| invalid(value.to_owned(), err.to_string()))
}
