#![forbid(unsafe_code)]

mod action;
mod datetime;
mod encoding;
mod error;
mod http_parts;
#[cfg(feature = "json")]
mod json;
mod range;
pub mod shape;

pub use satay_storage::{AllocStorage, BoxedStorage, Map, OwnedStorage, Storage};
#[cfg(feature = "json")]
pub use serde_json::Value as JsonValue;
pub use time::{Date, OffsetDateTime, PrimitiveDateTime, Time};
pub use url::Url;

pub use crate::action::{Action, BufferedResponse, OwnedAction, StorageAction};
pub use crate::datetime::{
    ParseDateError, ParseNaiveDateTimeError, ParseTimeError, format_date, format_naive_datetime,
    format_offset_datetime, format_time, format_unix_time, parse_date, parse_naive_datetime,
    parse_time,
};
pub use crate::encoding::{append_path_segment, append_query_pair, format_bool};
pub use crate::error::Error;
pub use crate::http_parts::{
    RequestParts, ResponseParts, has_json_content_type, insert_header, into_empty_request,
    into_request,
};
#[cfg(feature = "json")]
pub use crate::json::{
    from_json_slice, from_json_slice_in, from_projected_json_slice, into_json_request,
    into_optional_json_request,
};
pub use crate::range::{ParseRangeError, format_range, parse_range};

#[cfg(feature = "serde")]
#[allow(clippy::missing_errors_doc)]
pub mod serde_string;

#[cfg(feature = "serde")]
#[allow(clippy::missing_errors_doc)]
pub mod serde_integer;

#[cfg(feature = "json")]
pub mod treat_error_as_none;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "json"))]
mod buffered_response_tests;
