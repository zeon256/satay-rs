use std::fs;

use crate::common::*;

#[test]
fn inline_enum_generates_proper_enum_types() {
    let files = satay_codegen::generate(INLINE_ENUM).expect("generate inline-enum fixture");

    let types_rs = find_file(&files, "types.rs");
    assert!(types_rs.contents.contains("pub enum ItemCategory"));
    assert!(types_rs.contents.contains("pub enum ItemCondition"));
    assert!(types_rs.contents.contains("pub struct Item"));
    assert!(types_rs.contents.contains("#[default]"));
    assert!(types_rs.contents.contains("serde(other)"));
    assert!(!types_rs.contents.contains(r#"rename = """#));

    let item_struct_start = types_rs
        .contents
        .find("pub struct Item")
        .expect("Item struct exists");
    let item_struct = &types_rs.contents[item_struct_start..];
    assert!(item_struct.contains("category: ItemCategory"));
    assert!(item_struct.contains("condition: ItemCondition"));

    let category_enum_start = types_rs
        .contents
        .find("pub enum ItemCategory")
        .expect("ItemCategory enum exists");
    let category_enum = &types_rs.contents[category_enum_start..category_enum_start + 400];
    assert!(category_enum.contains("Electronics"));
    assert!(category_enum.contains("Clothing"));
    assert!(category_enum.contains("Food"));
    assert!(category_enum.contains("Unknown"));

    let condition_enum_start = types_rs
        .contents
        .find("pub enum ItemCondition")
        .expect("ItemCondition enum exists");
    let condition_enum = &types_rs.contents[condition_enum_start..condition_enum_start + 400];
    assert!(condition_enum.contains("New"));
    assert!(condition_enum.contains("Used"));
    assert!(condition_enum.contains("Refurbished"));
    assert!(condition_enum.contains("Unknown"));
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

    let types_rs = find_file(&files, "types.rs");
    let enum_start = types_rs
        .contents
        .find("pub enum BusArrivalTimingType")
        .expect("BusArrivalTimingType enum exists");
    let enum_contents = &types_rs.contents[enum_start..enum_start + 600];

    assert!(enum_contents.contains("SingleDecker"));
    assert!(enum_contents.contains("DoubleDecker"));
    assert!(enum_contents.contains("Bendy"));
    assert!(enum_contents.contains(r#"serde(rename = "SD")"#));
    assert!(enum_contents.contains(r#"serde(rename = "DD")"#));
    assert!(enum_contents.contains(r#"serde(rename = "BD")"#));
    assert!(enum_contents.contains("serde(other)"));
    assert!(!enum_contents.contains("Sd"));
    assert!(!enum_contents.contains("Dd"));
    assert!(!enum_contents.contains("Bd"));
    assert!(!enum_contents.contains(r#"rename = """#));
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
