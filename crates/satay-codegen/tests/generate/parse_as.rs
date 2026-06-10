use std::fs;

use crate::ast::*;
use crate::common::*;

#[test]
fn x_satay_parse_as_generates_wire_backed_deserializers() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /readings:
    get:
      operationId: getReading
      parameters:
        - name: readingId
          in: query
          required: true
          schema:
            $ref: '#/components/schemas/ReadingId'
      responses:
        '200':
          description: Reading
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Reading'
components:
  schemas:
    Reading:
      type: object
      required:
        - id
        - value
        - count
        - monitored
        - seenAt
        - startsAt
        - noServiceAt
        - aliasId
        - frequency
        - tolerance
      properties:
        id:
          type: string
          x-satay:
            parse-as: u32
        value:
          type: string
          x-satay:
            parse-as: f64
        count:
          type: string
          x-satay:
            parse-as: u8
        monitored:
          type: integer
          x-satay:
            parse-as: bool
        seenAt:
          type: string
          x-satay:
            parse-as: offset-datetime
        startsAt:
          type: [string, "null"]
          x-satay:
            parse-as: time
        noServiceAt:
          type: [string, "null"]
          x-satay:
            parse-as: time
        aliasId:
          $ref: '#/components/schemas/ReadingId'
        frequency:
          type: string
          minimum: 1
          maximum: 60
          x-satay:
            parse-as: integer-range
        tolerance:
          type: string
          format: double
          x-satay:
            parse-as: number-range
    ReadingId:
      type: string
      x-satay:
        parse-as: u32
"#,
    )
    .expect("generate parse-as fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    assert_eq!(
        norm(&find_type_alias(&types_rs, "ReadingId").ty),
        norm_str("u32")
    );
    let reading = find_struct(&types_rs, "Reading");
    assert_field(reading, "id", "u32");
    assert_field(reading, "value", "f64");
    assert_field(reading, "count", "u8");
    assert_field(reading, "monitored", "bool");
    assert_field(reading, "seen_at", "satay_runtime::OffsetDateTime");
    assert_field(reading, "starts_at", "Option<satay_runtime::Time>");
    assert_field(reading, "no_service_at", "Option<satay_runtime::Time>");
    assert_field(reading, "alias_id", "u32");
    assert_field(reading, "frequency", "ReadingFrequency");
    assert_field(reading, "tolerance", "ReadingTolerance");
    let frequency = find_struct(&types_rs, "ReadingFrequency");
    assert_field(frequency, "min", "Option<u8>");
    assert_field(frequency, "max", "Option<u8>");
    let tolerance = find_struct(&types_rs, "ReadingTolerance");
    assert_field(tolerance, "min", "Option<f64>");
    assert_attr_contains(
        &field(reading, "id").attrs,
        "cfg_attr",
        r#"with = "satay_runtime::serde_string::as_u32""#,
    );
    assert_attr_contains(
        &field(reading, "value").attrs,
        "cfg_attr",
        r#"with = "satay_runtime::serde_string::as_f64""#,
    );
    assert_attr_contains(
        &field(reading, "monitored").attrs,
        "cfg_attr",
        r#"with = "satay_runtime::serde_integer::as_bool""#,
    );
    assert_attr_contains(
        &field(reading, "seen_at").attrs,
        "cfg_attr",
        r#"with = "satay_runtime::serde_string::as_offset_datetime""#,
    );
    assert_attr_contains(
        &field(reading, "starts_at").attrs,
        "cfg_attr",
        r#"with = "satay_runtime::serde_string::as_time::option""#,
    );
    assert_attr_contains(
        &field(reading, "alias_id").attrs,
        "cfg_attr",
        r#"with = "satay_runtime::serde_string::as_u32""#,
    );

    let parts_rs = parse_rust(find_file(&files, "get_reading/parts.rs"));
    assert_field(
        find_struct(&parts_rs, "GetReadingInput"),
        "reading_id",
        "u32",
    );
    assert!(contains_tokens(
        find_fn(&parts_rs, "get_reading_parts"),
        "input.reading_id.to_string()"
    ));

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn decodes_and_encodes_string_backed_values() {
        let parts = get_reading_parts(GetReadingInput::new(42)).expect("request parts");
        assert_eq!(parts.uri, "/readings?readingId=42");

        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"id":"42","value":"1.25","count":"7","monitored":0,"seenAt":"2024-08-14T16:41:48+08:00","startsAt":"0620","noServiceAt":"","aliasId":"42","frequency":"14-17","tolerance":"1.5-2.75"}"#
                .to_vec(),
        };
        let decoded = decode_get_reading_response(response).expect("decoded response");

        match decoded {
            GetReadingResponse::Ok(reading) => {
                assert_eq!(reading.id, 42);
                assert_eq!(reading.value, 1.25);
                assert_eq!(reading.count, 7);
                assert!(!reading.monitored);
                assert_eq!(reading.seen_at.offset().whole_hours(), 8);
                let starts_at = reading.starts_at.expect("startsAt parsed");
                assert_eq!(starts_at.hour(), 6);
                assert_eq!(starts_at.minute(), 20);
                assert_eq!(reading.no_service_at, None);
                assert_eq!(reading.alias_id, 42);
                assert_eq!(reading.frequency.min, Some(14));
                assert_eq!(reading.frequency.max, Some(17));
                assert_eq!(reading.tolerance.min, Some(1.5));
                assert_eq!(reading.tolerance.max, Some(2.75));

                let encoded = serde_json::to_value(&reading).unwrap();
                assert_eq!(
                    encoded,
                    serde_json::json!({
                        "id": "42",
                        "value": "1.25",
                        "count": "7",
                        "monitored": 0,
                        "seenAt": "2024-08-14T16:41:48+08:00",
                        "startsAt": "0620",
                        "noServiceAt": null,
                        "aliasId": "42",
                        "frequency": "14-17",
                        "tolerance": "1.5-2.75"
                    })
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "parse-as generated crate tests");
}

#[test]
fn x_satay_parse_as_date_generates_query_parameter_encoding() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /psi:
    get:
      operationId: psi
      parameters:
        - name: date
          in: query
          schema:
            type: string
            x-satay:
              parse-as: date
      responses:
        '204':
          description: No content
"#,
    )
    .expect("generate parse-as date fixture");

    let parts_rs = parse_rust(find_file(&files, "psi/parts.rs"));
    assert_field(
        find_struct(&parts_rs, "PsiInput"),
        "date",
        "Option<satay_runtime::Date>",
    );
    assert!(contains_tokens(
        find_fn(&parts_rs, "psi_parts"),
        "satay_runtime::format_date(value)"
    ));

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn encodes_optional_date_query_parameter() {
        let day = satay_runtime::parse_date("2024-07-16").unwrap();
        let parts = psi_parts(PsiInput::new().date(day)).expect("request parts");
        assert_eq!(parts.uri, "/psi?date=2024-07-16");
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "parse-as date generated crate tests",
    );
}

#[test]
fn x_satay_parse_as_naive_datetime_generates_query_parameter_encoding() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /psi:
    get:
      operationId: psi
      parameters:
        - name: date
          in: query
          schema:
            type: string
            x-satay:
              parse-as: naive-datetime
      responses:
        '204':
          description: No content
"#,
    )
    .expect("generate parse-as naive-datetime fixture");

    let parts_rs = parse_rust(find_file(&files, "psi/parts.rs"));
    assert_field(
        find_struct(&parts_rs, "PsiInput"),
        "date",
        "Option<satay_runtime::PrimitiveDateTime>",
    );
    assert!(contains_tokens(
        find_fn(&parts_rs, "psi_parts"),
        "satay_runtime::format_naive_datetime(value)"
    ));

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn encodes_optional_naive_datetime_query_parameter() {
        let at = satay_runtime::parse_naive_datetime("2024-07-16T23:59:00").unwrap();
        let parts = psi_parts(PsiInput::new().date(at)).expect("request parts");
        assert_eq!(parts.uri, "/psi?date=2024-07-16T23%3A59%3A00");
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "parse-as naive-datetime generated crate tests",
    );
}

#[test]
fn unixtime_format_generates_offset_datetime_types_and_seconds_encoding() {
    let files = satay_codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /events:
    get:
      operationId: getEvents
      parameters:
        - name: at
          in: query
          required: true
          schema:
            type: integer
            format: unixtime
      responses:
        '204':
          description: No content
components:
  schemas:
    EventTime:
      type: integer
      format: unixtime
    Event:
      type: object
      required:
        - startedAt
        - endedAt
        - createdAtString
        - endedAtString
      properties:
        startedAt:
          type: integer
          format: unixtime
        endedAt:
          type: [integer, "null"]
          format: unixtime
        createdAtString:
          type: string
          format: unixtime
        endedAtString:
          type: [string, "null"]
          format: unixtime
"#,
    )
    .expect("generate unixtime fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    assert_eq!(
        norm(&find_type_alias(&types_rs, "EventTime").ty),
        norm_str("satay_runtime::OffsetDateTime")
    );
    let event = find_struct(&types_rs, "Event");
    assert_field(event, "started_at", "satay_runtime::OffsetDateTime");
    assert_field(event, "ended_at", "Option<satay_runtime::OffsetDateTime>");
    assert_field(event, "created_at_string", "satay_runtime::OffsetDateTime");
    assert_attr_contains(
        &field(event, "started_at").attrs,
        "cfg_attr",
        r#"with = "satay_runtime::serde_integer::as_unix_time""#,
    );
    assert_attr_contains(
        &field(event, "ended_at").attrs,
        "cfg_attr",
        r#"with = "satay_runtime::serde_integer::as_unix_time::option""#,
    );
    assert_attr_contains(
        &field(event, "created_at_string").attrs,
        "cfg_attr",
        r#"with = "satay_runtime::serde_string::as_unix_time""#,
    );
    assert_attr_contains(
        &field(event, "ended_at_string").attrs,
        "cfg_attr",
        r#"with = "satay_runtime::serde_string::as_unix_time::option""#,
    );

    let parts_rs = parse_rust(find_file(&files, "get_events/parts.rs"));
    assert_field(
        find_struct(&parts_rs, "GetEventsInput"),
        "at",
        "satay_runtime::OffsetDateTime",
    );
    assert!(contains_tokens(
        find_fn(&parts_rs, "get_events_parts"),
        "satay_runtime::format_unix_time(&input.at)"
    ));

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    let lib_contents = r##"pub mod generated;

#[cfg(test)]
mod tests {
    use super::generated::*;

    #[test]
    fn encodes_unixtime_query_parameter_and_json_values() {
        let at = satay_runtime::OffsetDateTime::from_unix_timestamp(1_719_892_800).unwrap();
        let before_epoch = satay_runtime::OffsetDateTime::from_unix_timestamp(-1).unwrap();

        let parts = get_events_parts(GetEventsInput::new(at)).expect("request parts");
        assert_eq!(parts.uri, "/events?at=1719892800");

        let event: Event = serde_json::from_value(serde_json::json!({
            "startedAt": 1719892800,
            "endedAt": null,
            "createdAtString": "1719892800",
            "endedAtString": "-1"
        }))
        .unwrap();

        assert_eq!(event.started_at, at);
        assert_eq!(event.ended_at, None);
        assert_eq!(event.created_at_string, at);
        assert_eq!(event.ended_at_string, Some(before_epoch));

        let encoded = serde_json::to_value(event).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "startedAt": 1719892800,
                "endedAt": null,
                "createdAtString": "1719892800",
                "endedAtString": "-1"
            })
        );
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "unixtime generated crate tests");
}
