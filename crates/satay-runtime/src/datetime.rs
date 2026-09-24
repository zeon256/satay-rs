use time::Month;
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime, PrimitiveDateTime, Time};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseTimeError {
    #[error("time must be exactly four ASCII digits in HHMM format")]
    InvalidFormat,

    #[error("time is outside valid HHMM range")]
    ComponentRange,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseDateError {
    #[error("date must be in YYYY-MM-DD format")]
    InvalidFormat,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseNaiveDateTimeError {
    #[error("datetime must be in YYYY-MM-DDTHH:mm:ss format")]
    InvalidFormat,
}
#[must_use]
pub fn format_offset_datetime(value: &OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| value.to_string())
}
#[must_use]
pub fn format_unix_time(value: &OffsetDateTime) -> String {
    value.unix_timestamp().to_string()
}
#[must_use]
pub fn format_date(value: &Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        value.year(),
        u8::from(value.month()),
        value.day()
    )
}
/// Parses a date in `YYYY-MM-DD` format.
///
/// # Errors
///
/// Returns an error if the value is not in `YYYY-MM-DD` format or is not a valid calendar date.
pub fn parse_date(value: &str) -> Result<Date, ParseDateError> {
    let value = value.trim().as_bytes();
    if value.len() != 10 || value[4] != b'-' || value[7] != b'-' {
        return Err(ParseDateError::InvalidFormat);
    }

    for (index, byte) in value.iter().enumerate() {
        if matches!(index, 4 | 7) {
            if *byte != b'-' {
                return Err(ParseDateError::InvalidFormat);
            }
        } else if !byte.is_ascii_digit() {
            return Err(ParseDateError::InvalidFormat);
        }
    }

    let year = parse_date_year(&value[0..4])?;
    let month = parse_date_u8(&value[5..7])?;
    let day = parse_date_u8(&value[8..10])?;
    let month = Month::try_from(month).map_err(|_| ParseDateError::InvalidFormat)?;
    Date::from_calendar_date(year, month, day).map_err(|_| ParseDateError::InvalidFormat)
}
fn parse_date_year(bytes: &[u8]) -> Result<i32, ParseDateError> {
    let mut value = 0i32;
    for byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(i32::from(*byte - b'0')))
            .ok_or(ParseDateError::InvalidFormat)?;
    }
    Ok(value)
}
fn parse_date_u8(bytes: &[u8]) -> Result<u8, ParseDateError> {
    let mut value = 0u16;
    for byte in bytes {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u16::from(*byte - b'0')))
            .ok_or(ParseDateError::InvalidFormat)?;
    }
    u8::try_from(value).map_err(|_| ParseDateError::InvalidFormat)
}
#[must_use]
pub fn format_naive_datetime(value: &PrimitiveDateTime) -> String {
    format!(
        "{}T{:02}:{:02}:{:02}",
        format_date(&value.date()),
        value.hour(),
        value.minute(),
        value.second()
    )
}
/// Parses a datetime in `YYYY-MM-DDTHH:mm:ss` format.
///
/// # Errors
///
/// Returns an error if the value is not in the expected format or has invalid date/time fields.
pub fn parse_naive_datetime(value: &str) -> Result<PrimitiveDateTime, ParseNaiveDateTimeError> {
    let value = value.trim();
    let bytes = value.as_bytes();
    if bytes.len() != 19
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return Err(ParseNaiveDateTimeError::InvalidFormat);
    }

    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 7 | 10 | 13 | 16) {
            continue;
        }
        if !byte.is_ascii_digit() {
            return Err(ParseNaiveDateTimeError::InvalidFormat);
        }
    }

    let date = parse_date(&value[0..10]).map_err(|_| ParseNaiveDateTimeError::InvalidFormat)?;
    let hour = parse_date_u8(&bytes[11..13]).map_err(|_| ParseNaiveDateTimeError::InvalidFormat)?;
    let minute =
        parse_date_u8(&bytes[14..16]).map_err(|_| ParseNaiveDateTimeError::InvalidFormat)?;
    let second =
        parse_date_u8(&bytes[17..19]).map_err(|_| ParseNaiveDateTimeError::InvalidFormat)?;
    let time =
        Time::from_hms(hour, minute, second).map_err(|_| ParseNaiveDateTimeError::InvalidFormat)?;
    Ok(PrimitiveDateTime::new(date, time))
}
/// Parses a time in `HHMM` format.
///
/// # Errors
///
/// Returns an error if the value is not four ASCII digits or is outside the valid time range.
pub fn parse_time(value: &str) -> Result<Time, ParseTimeError> {
    let value = value.trim();
    let bytes = value.as_bytes();
    if bytes.len() != 4 || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(ParseTimeError::InvalidFormat);
    }

    let hour = (bytes[0] - b'0') * 10 + (bytes[1] - b'0');
    let minute = (bytes[2] - b'0') * 10 + (bytes[3] - b'0');
    Time::from_hms(hour, minute, 0).map_err(|_| ParseTimeError::ComponentRange)
}
#[must_use]
pub fn format_time(value: &Time) -> String {
    format!("{:02}{:02}", value.hour(), value.minute())
}
