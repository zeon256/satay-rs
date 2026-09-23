use super::codegen;
use std::fs;

use super::ast::*;
use super::common::*;

#[test]
fn referenced_treat_error_as_none_fields_decode_lossily() {
    let files = codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Bus Arrival API
  version: 1.0.0
paths: {}
components:
  schemas:
    BusArrivalTiming:
      type: object
      required: [OriginCode, EstimatedArrival]
      properties:
        OriginCode:
          type: string
          x-satay:
            parse-as: u32
        EstimatedArrival:
          type: string
          x-satay:
            parse-as: offset-datetime
    BusServiceArrival:
      type: object
      required: [NextBus, NextBus2, NextBus3]
      properties:
        NextBus:
          $ref: '#/components/schemas/BusArrivalTiming'
          x-satay:
            treat-error-as-none: true
        NextBus2:
          $ref: '#/components/schemas/BusArrivalTiming'
          x-satay:
            treat-error-as-none: true
        NextBus3:
          $ref: '#/components/schemas/BusArrivalTiming'
          x-satay:
            treat-error-as-none: true
"#,
    )
    .expect("generate referenced treat-error-as-none fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let service = find_struct(&types_rs, "BusServiceArrival");
    for field_name in ["next_bus", "next_bus2", "next_bus3"] {
        assert_field(service, field_name, "Option<BusArrivalTiming>");
        assert_attr_contains(
            &field(service, field_name).attrs,
            "cfg_attr",
            r#"deserialize_with = "treat_error_as_none::deserialize""#,
        );
        assert_attr_contains(&field(service, field_name).attrs, "cfg_attr", "default");
        assert_attr_contains(
            &field(service, field_name).attrs,
            "cfg_attr",
            r#"skip_serializing_if = "Option::is_none""#,
        );
    }
    // Regression for `minimal_imports`: runtime modules arrive via cfg-gated
    // `use` items instead of fully qualified attribute paths. The module is
    // `json`-gated in satay-runtime, so its import carries both features.
    assert!(contains_tokens(
        &types_rs,
        "#[cfg(all(feature = \"serde\", feature = \"json\"))] use satay_runtime::treat_error_as_none;"
    ));

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");
    let runtime_path = runtime_path_toml();
    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/parse_as/referenced_treat_error_as_none_fields_decode_lossily/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "referenced treat-error-as-none generated crate tests",
    );
}

#[test]
fn x_satay_parse_as_generates_wire_backed_deserializers() {
    let files = codegen::generate(
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
        r#"with = "serde_string::as_u32""#,
    );
    assert_attr_contains(
        &field(reading, "value").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_f64""#,
    );
    assert_attr_contains(
        &field(reading, "monitored").attrs,
        "cfg_attr",
        r#"with = "serde_integer::as_bool""#,
    );
    assert_attr_contains(
        &field(reading, "seen_at").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_offset_datetime""#,
    );
    assert_attr_contains(
        &field(reading, "starts_at").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_time::option""#,
    );
    assert_attr_contains(
        &field(reading, "alias_id").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_u32""#,
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
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/parse_as/x_satay_parse_as_generates_wire_backed_deserializers/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "parse-as generated crate tests");
}

#[test]
fn x_satay_none_if_generates_strict_optional_parsed_fields() {
    let files = codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    Reading:
      type: object
      required: [requiredWbgt, nullableWbgt, maximumSpeed]
      properties:
        requiredWbgt:
          type: string
          x-satay:
            parse-as: f64
            none-if: [NA, "-"]
        optionalWbgt:
          type: string
          x-satay:
            parse-as: f64
            none-if: [NA]
        nullableWbgt:
          type: [string, "null"]
          x-satay:
            parse-as: f64
            none-if: [NA]
        maximumSpeed:
          type: string
          x-satay:
            parse-as: u8
            none-if: ["999"]
"#,
    )
    .expect("generate none-if fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let reading = find_struct(&types_rs, "Reading");
    assert_field(reading, "required_wbgt", "Option<f64>");
    assert_field(reading, "optional_wbgt", "Option<f64>");
    assert_field(reading, "nullable_wbgt", "Option<f64>");
    assert_field(reading, "maximum_speed", "Option<u8>");
    assert_attr_contains(
        &field(reading, "required_wbgt").attrs,
        "cfg_attr",
        r#"deserialize_with = "Reading::__satay_deserialize_required_wbgt_none_if""#,
    );
    assert_attr_contains(
        &field(reading, "optional_wbgt").attrs,
        "cfg_attr",
        "default",
    );
    assert_attr_contains(
        &field(reading, "optional_wbgt").attrs,
        "cfg_attr",
        r#"skip_serializing_if = "Option::is_none""#,
    );
    // Regression for `minimal_imports`: call sites stay at two segments and
    // the runtime serde modules arrive via cfg-gated `use` items.
    assert!(contains_tokens(&types_rs, "as_f64::deserialize_none_if"));
    assert!(contains_tokens(&types_rs, "&[\"NA\", \"-\"]"));
    assert!(contains_tokens(
        &types_rs,
        "as_f64_option::deserialize_none_if"
    ));
    assert!(contains_tokens(
        &types_rs,
        "as_f64::serialize_none_if(value, \"NA\", serializer)"
    ));
    assert!(contains_tokens(
        &types_rs,
        r#"#[allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref, reason = "Serde `serialize_with` receives a reference to the field type")] fn __satay_serialize_required_wbgt_none_if"#
    ));
    assert!(contains_tokens(&types_rs, "as_u8::deserialize_none_if"));
    assert!(contains_tokens(&types_rs, "&[\"999\"]"));
    assert!(contains_tokens(
        &types_rs,
        r#"as_u8::serialize_none_if(value, "999", serializer)"#
    ));
    assert!(contains_tokens(
        &types_rs,
        r#"#[allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref, reason = "Serde `serialize_with` receives a reference to the field type")] fn __satay_serialize_maximum_speed_none_if"#
    ));
    assert!(contains_tokens(
        &types_rs,
        "#[cfg(feature = \"serde\")] use satay_runtime::serde_string::{as_f64, as_u8};"
    ));
    assert!(contains_tokens(
        &types_rs,
        "#[cfg(feature = \"serde\")] use satay_runtime::serde_string::as_f64::option as as_f64_option;"
    ));

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");
    let runtime_path = runtime_path_toml();
    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/parse_as/x_satay_none_if_generates_strict_optional_parsed_fields/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "none-if generated crate tests");
    run_temp_cargo(
        crate_dir,
        "check",
        &["--no-default-features", "--features", "serde"],
        "none-if serde-only generated crate check",
    );
    run_temp_cargo(
        crate_dir,
        "clippy",
        &[
            "--lib",
            "--",
            "-D",
            "clippy::ref_option",
            "-D",
            "clippy::trivially_copy_pass_by_ref",
        ],
        "none-if generated crate scoped serializer clippy",
    );
}

#[test]
fn x_satay_bool_mappings_generate_configured_serde_behavior() {
    let files = codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Test API
  version: 1.0.0
paths:
  /indicators:
    get:
      operationId: getIndicators
      parameters:
        - name: enabled
          in: query
          required: true
          schema:
            $ref: '#/components/schemas/MappedBool'
      responses:
        '204':
          description: No content
components:
  schemas:
    Indicators:
      type: object
      required: [strict, fallback, lossy, requiredNullable, noneMapped]
      properties:
        strict:
          type: string
          x-satay:
            parse-as: bool
            true-values: [Y, Yes, "1", "true"]
            false-values: [N, No, "0", "false", ""]
        fallback:
          type: string
          x-satay:
            parse-as: bool
            true-values: [Y]
            false-values: [N]
            unknown-as: false
        lossy:
          type: string
          x-satay:
            parse-as: bool
            true-values: [Y]
            false-values: [N]
            treat-error-as-none: true
        optional:
          type: [string, "null"]
          x-satay:
            parse-as: bool
            true-values: [Y]
            false-values: [N]
        requiredNullable:
          type: [string, "null"]
          x-satay:
            parse-as: bool
            true-values: [Y]
            false-values: [N]
        reusable:
          $ref: '#/components/schemas/MappedBool'
        noneMapped:
          type: string
          x-satay:
            parse-as: bool
            true-values: [Y]
            false-values: [N]
            none-if: [""]
    MappedBool:
      type: string
      x-satay:
        parse-as: bool
        true-values: [Y]
        false-values: [N]
"#,
    )
    .expect("generate configured bool fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let indicators = find_struct(&types_rs, "Indicators");
    assert_field(indicators, "strict", "bool");
    assert_field(indicators, "fallback", "bool");
    assert_field(indicators, "lossy", "Option<bool>");
    assert_field(indicators, "optional", "Option<bool>");
    assert_field(indicators, "required_nullable", "Option<bool>");
    assert_field(indicators, "reusable", "Option<bool>");
    assert_field(indicators, "none_mapped", "Option<bool>");
    assert_attr_contains(
        &field(indicators, "strict").attrs,
        "cfg_attr",
        r#"deserialize_with = "Indicators::__satay_deserialize_strict_bool_mapping""#,
    );
    assert!(contains_tokens(&types_rs, r#"&["Y", "Yes", "1", "true"]"#));
    assert!(contains_tokens(
        &types_rs,
        r#"&["N", "No", "0", "false", ""]"#
    ));
    // Regression for `minimal_imports`: call sites stay at two segments and
    // the runtime serde modules arrive via cfg-gated `use` items.
    assert!(contains_tokens(
        &types_rs,
        "as_bool_option::deserialize_mapped"
    ));
    assert!(contains_tokens(
        &types_rs,
        "as_bool::serialize_mapped_none_if"
    ));
    assert!(contains_tokens(
        &types_rs,
        "#[cfg(feature = \"serde\")] use satay_runtime::serde_string::as_bool;"
    ));
    assert!(contains_tokens(
        &types_rs,
        "#[cfg(feature = \"serde\")] use satay_runtime::serde_string::as_bool::option as as_bool_option;"
    ));

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");
    let runtime_path = runtime_path_toml();
    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/parse_as/x_satay_bool_mappings_generate_configured_serde_behavior/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "configured bool generated crate tests",
    );
}

#[test]
fn x_satay_parse_as_date_generates_query_parameter_encoding() {
    let files = codegen::generate(
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
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/parse_as/x_satay_parse_as_date_generates_query_parameter_encoding/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
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
    let files = codegen::generate(
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
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/parse_as/x_satay_parse_as_naive_datetime_generates_query_parameter_encoding/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
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
    let files = codegen::generate(
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
        r#"with = "serde_integer::as_unix_time""#,
    );
    assert_attr_contains(
        &field(event, "ended_at").attrs,
        "cfg_attr",
        r#"with = "serde_integer::as_unix_time::option""#,
    );
    assert_attr_contains(
        &field(event, "created_at_string").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_unix_time""#,
    );
    assert_attr_contains(
        &field(event, "ended_at_string").attrs,
        "cfg_attr",
        r#"with = "serde_string::as_unix_time::option""#,
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
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/parse_as/unixtime_format_generates_offset_datetime_types_and_seconds_encoding/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "unixtime generated crate tests");
}
