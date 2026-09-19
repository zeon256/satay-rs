use super::*;
#[cfg(all(feature = "serde", feature = "json"))]
mod required_f64_none_if;

#[cfg(all(feature = "serde", feature = "json"))]
mod optional_f64_none_if;

#[test]
fn encodes_path_segments() {
    let mut out = String::new();
    append_path_segment(&mut out, "a/b c");
    assert_eq!(out, "a%2Fb%20c");
}

#[test]
fn appends_query_pairs() {
    let mut out = String::from("/pets");
    let mut first = true;
    append_query_pair(&mut out, &mut first, "tag name", "small/dog");
    append_query_pair(&mut out, &mut first, "limit", "10");
    assert_eq!(out, "/pets?tag%20name=small%2Fdog&limit=10");
}

#[test]
fn parses_range_strings() {
    assert_eq!(parse_range::<u8>("14-17").unwrap(), (Some(14), Some(17)));
    assert_eq!(parse_range::<u8>("14-").unwrap(), (Some(14), None));
    assert_eq!(parse_range::<u8>("-17").unwrap(), (None, Some(17)));
    assert_eq!(parse_range::<u8>("").unwrap(), (None, None));
    assert!(matches!(
        parse_range::<u8>("14-17-20"),
        Err(ParseRangeError::TooManySeparators)
    ));
}

#[test]
fn formats_range_strings() {
    assert_eq!(format_range(&Some(14), &Some(17)), "14-17");
    assert_eq!(format_range(&Some(14), &None::<u8>), "14-");
    assert_eq!(format_range(&None::<u8>, &Some(17)), "-17");
    assert_eq!(format_range(&None::<u8>, &None::<u8>), "");
}

#[test]
fn parses_and_formats_time_strings() {
    let time = parse_time("0620").unwrap();
    assert_eq!(time.hour(), 6);
    assert_eq!(time.minute(), 20);
    assert_eq!(format_time(&time), "0620");
    assert_eq!(parse_time("6:20"), Err(ParseTimeError::InvalidFormat));
    assert_eq!(parse_time("2400"), Err(ParseTimeError::ComponentRange));
}

#[test]
fn parses_and_formats_date_strings() {
    let date = parse_date("2024-07-16").unwrap();
    assert_eq!(
        date,
        Date::from_calendar_date(2024, Month::July, 16).unwrap()
    );
    assert_eq!(format_date(&date), "2024-07-16");
    assert_eq!(parse_date("2024-7-16"), Err(ParseDateError::InvalidFormat));
    assert_eq!(parse_date("not-a-date"), Err(ParseDateError::InvalidFormat));
}

#[test]
fn parses_and_formats_naive_datetime_strings() {
    let datetime = parse_naive_datetime("2024-07-16T23:59:00").unwrap();
    assert_eq!(datetime.hour(), 23);
    assert_eq!(datetime.minute(), 59);
    assert_eq!(datetime.second(), 0);
    assert_eq!(format_naive_datetime(&datetime), "2024-07-16T23:59:00");
    assert_eq!(
        parse_naive_datetime("2024-07-16T23:59"),
        Err(ParseNaiveDateTimeError::InvalidFormat)
    );
    assert_eq!(
        parse_naive_datetime("2024-07-16 23:59:00"),
        Err(ParseNaiveDateTimeError::InvalidFormat)
    );
}

#[test]
fn formats_unix_time_seconds() {
    let datetime = OffsetDateTime::from_unix_timestamp(-1).unwrap();
    assert_eq!(format_unix_time(&datetime), "-1");
}

#[cfg(all(feature = "serde", feature = "json"))]
#[test]
fn serde_string_naive_datetime_round_trips() {
    #[derive(serde::Deserialize, serde::Serialize)]
    struct Value {
        #[serde(with = "crate::serde_string::as_naive_datetime")]
        at: PrimitiveDateTime,
    }

    let at = parse_naive_datetime("2024-07-16T23:59:00").unwrap();
    let encoded = serde_json::to_value(Value { at }).unwrap();
    assert_eq!(encoded, serde_json::json!({ "at": "2024-07-16T23:59:00" }));

    let decoded = serde_json::from_value::<Value>(encoded).unwrap();
    assert_eq!(decoded.at, at);
}

#[cfg(all(feature = "serde", feature = "json"))]
#[test]
fn serde_string_url_round_trips_and_rejects_invalid_values() {
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct Links {
        #[serde(with = "crate::serde_string::as_url")]
        url: Url,
        #[serde(default, with = "crate::serde_string::as_url::option")]
        optional: Option<Url>,
    }

    let links: Links = serde_json::from_str(
        r#"{"url":"https://EXAMPLE.com:443/path","optional":"mailto:hello@example.com"}"#,
    )
    .unwrap();
    assert_eq!(links.url.as_str(), "https://example.com/path");
    assert_eq!(links.optional.as_ref().unwrap().scheme(), "mailto");
    assert_eq!(
        serde_json::to_value(&links).unwrap()["url"],
        "https://example.com/path"
    );
    for optional in ["null", "\"/relative\"", "42"] {
        let json = format!(r#"{{"url":"https://example.com/","optional":{optional}}}"#);
        let result = serde_json::from_str::<Links>(&json);
        if optional == "null" {
            assert!(result.unwrap().optional.is_none());
        } else {
            assert!(result.is_err());
        }
    }
    assert!(serde_json::from_str::<Links>(r#"{"url":"/relative"}"#).is_err());
    assert!(serde_json::from_str::<Links>(r#"{"url":42}"#).is_err());
}

#[cfg(all(feature = "serde", feature = "json"))]
#[test]
fn serde_string_date_round_trips() {
    #[derive(serde::Deserialize, serde::Serialize)]
    struct Value {
        #[serde(with = "crate::serde_string::as_date")]
        day: Date,
    }

    let date = Date::from_calendar_date(2024, Month::July, 16).unwrap();
    let encoded = serde_json::to_value(Value { day: date }).unwrap();
    assert_eq!(encoded, serde_json::json!({ "day": "2024-07-16" }));

    let decoded = serde_json::from_value::<Value>(encoded).unwrap();
    assert_eq!(decoded.day, date);
}

#[test]
fn recognizes_json_content_types() {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        http::HeaderValue::from_static("application/problem+json; charset=utf-8"),
    );
    assert!(has_json_content_type(&headers));
}

#[test]
fn response_parts_holds_status_headers_body() {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    let body = br#"{"ok":true}"#.to_vec();
    let parts = ResponseParts {
        status: http::StatusCode::OK,
        headers,
        body,
    };
    assert_eq!(parts.status, http::StatusCode::OK);
    assert_eq!(parts.headers.get(CONTENT_TYPE).unwrap(), "application/json");
    assert_eq!(parts.body, br#"{"ok":true}"#);
}

#[cfg(feature = "json")]
#[test]
fn projected_json_unwraps_and_maps_fields() {
    let body = br#"{
            "odata.metadata": "https://example.test/metadata",
            "value": [
                {"Link": "https://example.test/a", "Name": "A"},
                {"Link": "https://example.test/b", "Name": "B"}
            ]
        }"#;

    let rows = from_projected_json_slice::<Vec<JsonValue>>(body, "value", None).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["Name"], "A");

    let links = from_projected_json_slice::<Vec<String>>(body, "value", Some("Link")).unwrap();
    assert_eq!(
        links,
        vec![
            "https://example.test/a".to_owned(),
            "https://example.test/b".to_owned()
        ]
    );
}

#[cfg(feature = "json")]
#[test]
fn projected_json_preserves_optional_missing_fields_as_null() {
    let missing = br#"{"metadata":"present"}"#;
    let value = from_projected_json_slice::<Option<Vec<String>>>(missing, "value", None)
        .expect("optional missing projection");
    assert_eq!(value, None);

    let rows = br#"{"value":[{}, {"Link":"present"}]}"#;
    let links = from_projected_json_slice::<Vec<Option<String>>>(rows, "value", Some("Link"))
        .expect("optional mapped field");
    assert_eq!(links, vec![None, Some("present".to_owned())]);
}

#[cfg(feature = "json")]
#[test]
fn projected_json_rejects_invalid_container_shapes() {
    let scalar = from_projected_json_slice::<Vec<String>>(
        br#"{"value":"not-an-array"}"#,
        "value",
        Some("Link"),
    );
    assert!(matches!(scalar, Err(Error::InvalidResponse(_))));

    let scalar_item = from_projected_json_slice::<Vec<String>>(
        br#"{"value":["not-an-object"]}"#,
        "value",
        Some("Link"),
    );
    assert!(matches!(scalar_item, Err(Error::InvalidResponse(_))));
}

#[cfg(all(feature = "serde", feature = "json"))]
#[test]
fn serde_string_bool_accepts_string_and_numeric_values() {
    #[derive(serde::Deserialize, serde::Serialize)]
    struct Value {
        #[serde(with = "crate::serde_string::as_bool")]
        monitored: bool,
    }

    let numeric = serde_json::from_str::<Value>(r#"{"monitored":0}"#).unwrap();
    assert!(!numeric.monitored);

    let string = serde_json::from_str::<Value>(r#"{"monitored":"1"}"#).unwrap();
    assert!(string.monitored);

    let encoded = serde_json::to_value(Value { monitored: false }).unwrap();
    assert_eq!(encoded, serde_json::json!({ "monitored": "0" }));
}

#[cfg(all(feature = "serde", feature = "json"))]
#[test]
fn serde_string_none_if_is_strict_and_canonical() {
    #[derive(Debug, serde::Deserialize, serde::Serialize)]
    struct Value {
        #[serde(with = "required_f64_none_if")]
        required: Option<f64>,
        #[serde(default, with = "optional_f64_none_if")]
        optional: Option<f64>,
    }

    let valid = serde_json::from_str::<Value>(r#"{"required":"28.7","optional":"10.5"}"#).unwrap();
    assert_eq!(valid.required, Some(28.7));
    assert_eq!(valid.optional, Some(10.5));

    let sentinel = serde_json::from_str::<Value>(r#"{"required":"-","optional":"NA"}"#).unwrap();
    assert_eq!(sentinel.required, None);
    assert_eq!(sentinel.optional, None);

    let null_optional =
        serde_json::from_str::<Value>(r#"{"required":"28.7","optional":null}"#).unwrap();
    assert_eq!(null_optional.optional, None);
    assert!(serde_json::from_str::<Value>(r#"{"required":null}"#).is_err());
    assert!(serde_json::from_str::<Value>(r#"{"required":"unexpected"}"#).is_err());

    let encoded = serde_json::to_value(Value {
        required: None,
        optional: Some(10.5),
    })
    .unwrap();
    assert_eq!(
        encoded,
        serde_json::json!({"required": "NA", "optional": "10.5"})
    );
}

#[cfg(all(feature = "serde", feature = "json"))]
#[test]
fn serde_string_none_if_preserves_bool_and_time_parsers() {
    use crate::serde_string::{as_bool, as_time::option as time_option};
    use serde_json::Value;

    let bool_sentinel =
        as_bool::deserialize_none_if(Value::String("NA".to_owned()), &["NA"]).unwrap();
    assert_eq!(bool_sentinel, None);

    let bool_numeric = as_bool::deserialize_none_if(Value::from(1), &["NA"]).unwrap();
    assert_eq!(bool_numeric, Some(true));

    let empty_time =
        time_option::deserialize_none_if(Value::String("  ".to_owned()), &["-"]).unwrap();
    assert_eq!(empty_time, None);
    assert!(time_option::deserialize_none_if(Value::String("invalid".to_owned()), &["-"]).is_err());
}

#[cfg(all(feature = "serde", feature = "json"))]
#[test]
fn serde_integer_bool_accepts_numeric_values() {
    #[derive(serde::Deserialize, serde::Serialize)]
    struct Value {
        #[serde(with = "crate::serde_integer::as_bool")]
        monitored: bool,
    }

    let numeric = serde_json::from_str::<Value>(r#"{"monitored":0}"#).unwrap();
    assert!(!numeric.monitored);

    let encoded = serde_json::to_value(Value { monitored: true }).unwrap();
    assert_eq!(encoded, serde_json::json!({ "monitored": 1 }));
}

#[cfg(all(feature = "serde", feature = "json"))]
#[test]
fn serde_string_unix_time_round_trips() {
    #[derive(serde::Deserialize, serde::Serialize)]
    struct Value {
        #[serde(with = "crate::serde_string::as_unix_time")]
        at: OffsetDateTime,
    }

    let at = OffsetDateTime::from_unix_timestamp(1_719_892_800).unwrap();
    let encoded = serde_json::to_value(Value { at }).unwrap();
    assert_eq!(encoded, serde_json::json!({ "at": "1719892800" }));

    let decoded = serde_json::from_value::<Value>(encoded).unwrap();
    assert_eq!(decoded.at, at);
}

#[cfg(all(feature = "serde", feature = "json"))]
#[test]
fn serde_integer_unix_time_round_trips_and_handles_null() {
    #[derive(serde::Deserialize, serde::Serialize)]
    struct Value {
        #[serde(with = "crate::serde_integer::as_unix_time")]
        at: OffsetDateTime,
        #[serde(with = "crate::serde_integer::as_unix_time::option")]
        maybe_at: Option<OffsetDateTime>,
    }

    let at = OffsetDateTime::from_unix_timestamp(1_719_892_800).unwrap();
    let encoded = serde_json::to_value(Value { at, maybe_at: None }).unwrap();
    assert_eq!(
        encoded,
        serde_json::json!({ "at": 1_719_892_800, "maybe_at": null })
    );

    let decoded = serde_json::from_value::<Value>(encoded).unwrap();
    assert_eq!(decoded.at, at);
    assert_eq!(decoded.maybe_at, None);
}

#[cfg(all(feature = "serde", feature = "json"))]
#[test]
fn serde_integer_unix_time_rejects_out_of_range_values() {
    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct Value {
        #[serde(with = "crate::serde_integer::as_unix_time")]
        at: OffsetDateTime,
    }

    assert!(serde_json::from_str::<Value>(r#"{"at":9223372036854775807}"#).is_err());
}
