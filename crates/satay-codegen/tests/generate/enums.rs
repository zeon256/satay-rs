use super::codegen;
use std::fs;

use super::ast::*;
use super::common::*;

#[test]
fn inline_enum_generates_proper_enum_types() {
    let files = codegen::generate(INLINE_ENUM).expect("generate inline-enum fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let item = find_struct(&types_rs, "Item");
    assert_field(item, "category", "ItemCategory");
    assert_field(item, "condition", "ItemCondition");

    let category = find_enum(&types_rs, "ItemCategory");
    assert_eq!(variant_names(category), ["Electronics", "Clothing", "Food"]);
    let category_as_str = find_method(&types_rs, "ItemCategory", "as_str");
    assert!(is_pub(&category_as_str.vis));
    assert_eq!(
        norm(&category_as_str.sig),
        norm_str("const fn as_str(&self) -> &'static str")
    );

    let condition = find_enum(&types_rs, "ItemCondition");
    assert_eq!(variant_names(condition), ["New", "Used", "Refurbished"]);
    assert!(!contains_tokens(&types_rs, "serde ( other )"));
}

#[test]
fn x_satay_enum_variants_generate_named_variants() {
    let files = codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Enum Variants API
  version: 1.0.0
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BusArrivalTiming'
components:
  schemas:
    BusArrivalTiming:
      type: object
      required:
        - Type
      properties:
        Type:
          type: string
          description: Vehicle type.
          enum:
            - SD
            - DD
            - BD
            - ""
          example: SD
          x-satay:
            enum-variants:
              SD: SingleDecker
              DD: DoubleDecker
              BD: Bendy
              "": Unknown
"#,
    )
    .expect("generate enum variants fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let timing = find_enum(&types_rs, "BusArrivalTimingType");
    assert_eq!(
        variant_names(timing),
        ["SingleDecker", "DoubleDecker", "Bendy", "Unknown"]
    );
    assert_attr_contains(
        &variant(timing, "SingleDecker").attrs,
        "cfg_attr",
        r#"serde(rename = "SD")"#,
    );
    assert_attr_contains(
        &variant(timing, "DoubleDecker").attrs,
        "cfg_attr",
        r#"serde(rename = "DD")"#,
    );
    assert_attr_contains(
        &variant(timing, "Bendy").attrs,
        "cfg_attr",
        r#"serde(rename = "BD")"#,
    );
    assert_attr_contains(
        &variant(timing, "Unknown").attrs,
        "cfg_attr",
        r#"serde(rename = "")"#,
    );
    assert!(!contains_tokens(&types_rs, "serde ( other )"));
}

#[test]
fn closed_enum_can_generate_other_variant_for_other_wire_value() {
    let files = codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Reason API
  version: 1.0.0
paths:
  /reason:
    get:
      operationId: getReason
      responses:
        '200':
          description: Reason
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Reason'
components:
  schemas:
    Reason:
      type: string
      enum:
        - content_filter
        - other
"#,
    )
    .expect("generate closed enum with other fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let reason = find_enum(&types_rs, "Reason");
    assert_eq!(variant_names(reason), ["ContentFilter", "Other"]);
}

#[test]
fn any_of_string_and_enum_generates_open_string_enum() {
    let files = codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Audio API
  version: 1.0.0
paths:
  /transcription:
    get:
      operationId: getTranscription
      responses:
        '200':
          description: Transcription
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AudioTranscription'
components:
  schemas:
    AudioTranscription:
      type: object
      properties:
        model:
          anyOf:
            - type: string
            - type: string
              description: Known transcription models.
              enum:
                - whisper-1
                - gpt-4o-mini-transcribe
                - gpt-4o-mini-transcribe-2025-12-15
                - gpt-4o-transcribe
                - gpt-4o-transcribe-diarize
                - gpt-realtime-whisper
"#,
    )
    .expect("generate open string enum fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let transcription = find_struct(&types_rs, "AudioTranscription");
    assert_field(
        transcription,
        "model",
        "Option<AudioTranscriptionModel<'storage, S>>",
    );

    let model = find_enum(&types_rs, "AudioTranscriptionModel");
    assert_doc(&model.attrs, "Known transcription models.");
    assert_eq!(
        variant_names(model),
        [
            "Whisper1",
            "Gpt4oMiniTranscribe",
            "Gpt4oMiniTranscribe20251215",
            "Gpt4oTranscribe",
            "Gpt4oTranscribeDiarize",
            "GptRealtimeWhisper",
            "Other"
        ]
    );
    let model_as_str = find_method(&types_rs, "AudioTranscriptionModel", "as_str");
    assert!(is_pub(&model_as_str.vis));
    assert_eq!(
        norm(&model_as_str.sig),
        norm_str("fn as_str(&self) -> &str")
    );
    assert_eq!(
        norm_fields(&variant(model, "Other").fields),
        norm_str("(<S as satay_runtime::storage::Storage>::Text<'storage>)")
    );
    assert!(contains_tokens(
        &types_rs,
        "serde::Serialize for AudioTranscriptionModel<'storage, S>"
    ));
    assert!(contains_tokens(
        &types_rs,
        "serde::Deserialize<'de> for AudioTranscriptionModel<'storage, S>"
    ));
    // Regression for #155: must not emit the long UFCS path that fires the
    // `minimal_imports` lint; reference `Deserialize` unqualified instead.
    assert!(!contains_tokens(
        &types_rs,
        "serde :: Deserialize :: deserialize"
    ));
    assert!(contains_tokens(
        &types_rs,
        "<<S as satay_runtime::storage::Storage>::Text<'storage> as serde::Deserialize>::deserialize(deserializer)"
    ));
}

#[test]
fn any_of_const_branches_generate_open_string_enum() {
    let files = codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Messages API
  version: 1.0.0
paths:
  /model:
    get:
      operationId: getModel
      responses:
        '200':
          description: Model
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Model'
components:
  schemas:
    Model:
      anyOf:
        - type: string
        - const: claude-sonnet-5
        - const: claude-haiku-4-5
"#,
    )
    .expect("generate open string enum from const branches fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let model = find_enum(&types_rs, "Model");
    assert_eq!(
        variant_names(model),
        ["ClaudeSonnet5", "ClaudeHaiku45", "Other"]
    );
    assert_eq!(
        norm_fields(&variant(model, "Other").fields),
        norm_str("(<S as satay_runtime::storage::Storage>::Text<'storage>)")
    );
    assert!(contains_tokens(
        &types_rs,
        r#"Self::ClaudeSonnet5 => "claude-sonnet-5""#
    ));
    assert!(contains_tokens(
        &types_rs,
        r#"Self::ClaudeHaiku45 => "claude-haiku-4-5""#
    ));
}

#[test]
fn open_string_enum_mangles_other_known_value_and_keeps_fallback() {
    let files = codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Event API
  version: 1.0.0
paths:
  /event:
    get:
      operationId: getEvent
      responses:
        '200':
          description: Event
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Event'
components:
  schemas:
    Event:
      type: object
      properties:
        reason:
          anyOf:
            - type: string
            - type: string
              enum:
                - content_filter
                - other
"#,
    )
    .expect("generate open string enum with other fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let event = find_struct(&types_rs, "Event");
    assert_field(event, "reason", "Option<EventReason<'storage, S>>");

    let reason = find_enum(&types_rs, "EventReason");
    assert_eq!(variant_names(reason), ["ContentFilter", "Other_2", "Other"]);
    assert_eq!(
        norm_fields(&variant(reason, "Other").fields),
        norm_str("(<S as satay_runtime::storage::Storage>::Text<'storage>)")
    );
}

#[test]
fn generated_inline_enum_compiles_and_rejects_unknown() {
    let files = codegen::generate(INLINE_ENUM).expect("generate inline-enum fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!("tests/enums/generated_inline_enum_compiles_and_rejects_unknown/tests.rs"),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "inline-enum generated crate tests");
}

#[test]
fn generated_open_string_enum_preserves_unknown_values() {
    let files = codegen::generate(
        r#"
openapi: 3.1.0
info:
  title: Audio API
  version: 1.0.0
paths:
  /transcription:
    get:
      operationId: getTranscription
      responses:
        '200':
          description: Transcription
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AudioTranscription'
components:
  schemas:
    AudioTranscription:
      type: object
      properties:
        model:
          anyOf:
            - type: string
            - type: string
              enum:
                - whisper-1
                - gpt-4o-mini-transcribe
                - gpt-4o-transcribe
"#,
    )
    .expect("generate open string enum runtime fixture");

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!("tests/enums/generated_open_string_enum_preserves_unknown_values/tests.rs"),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "open string enum generated crate tests",
    );
}
