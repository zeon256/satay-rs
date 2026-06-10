use std::fs;

use crate::ast::*;
use crate::common::*;

#[test]
fn inline_enum_generates_proper_enum_types() {
    let files = satay_codegen::generate(INLINE_ENUM).expect("generate inline-enum fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    let item = find_struct(&types_rs, "Item");
    assert_field(item, "category", "ItemCategory");
    assert_field(item, "condition", "ItemCondition");
    assert!(!contains_tokens(&types_rs, r#"rename = """#));

    let category = find_enum(&types_rs, "ItemCategory");
    assert_eq!(
        variant_names(category),
        ["Electronics", "Clothing", "Food", "Unknown"]
    );
    let unknown = variant(category, "Unknown");
    assert!(has_attr(&unknown.attrs, "default"));
    assert_attr_contains(&unknown.attrs, "cfg_attr", "serde(other)");

    let condition = find_enum(&types_rs, "ItemCondition");
    assert_eq!(
        variant_names(condition),
        ["New", "Used", "Refurbished", "Unknown"]
    );
    let unknown = variant(condition, "Unknown");
    assert!(has_attr(&unknown.attrs, "default"));
    assert_attr_contains(&unknown.attrs, "cfg_attr", "serde(other)");
}

#[test]
fn x_satay_enum_variants_generate_named_variants() {
    let files = satay_codegen::generate(
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
        "serde(other)",
    );
    assert!(!contains_tokens(&types_rs, r#"rename = """#));
}

#[test]
fn generated_inline_enum_compiles_and_handles_unknown() {
    let files = satay_codegen::generate(INLINE_ENUM).expect("generate inline-enum fixture");

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
    fn known_enum_variants_deserialize() {
        let json = br#"{"id":"1","name":"Widget","category":"electronics","condition":"new","notes":"test"}"#.to_vec();
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: json,
        };
        let decoded = decode_get_item_response(response).expect("decoded response");
        match decoded {
            GetItemResponse::Ok(item) => {
                assert_eq!(item.id, "1");
                assert_eq!(item.name, "Widget");
                assert_eq!(item.category, ItemCategory::Electronics);
                assert_eq!(item.condition, ItemCondition::New);
                assert_eq!(item.notes, Some("test".to_owned()));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn unknown_enum_variant_maps_to_unknown() {
        let json = br#"{"id":"2","name":"Gadget","category":"unknown_category","condition":"","notes":null}"#.to_vec();
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: json,
        };
        let decoded = decode_get_item_response(response).expect("decoded response");
        match decoded {
            GetItemResponse::Ok(item) => {
                assert_eq!(item.category, ItemCategory::Unknown);
                assert_eq!(item.condition, ItemCondition::Unknown);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn default_variant_is_unknown() {
        assert_eq!(ItemCategory::default(), ItemCategory::Unknown);
        assert_eq!(ItemCondition::default(), ItemCondition::Unknown);
    }
}
"##;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "inline-enum generated crate tests");
}
