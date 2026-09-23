use super::generated::*;

const CATEGORY: &str = ItemCategory::Electronics.as_str();

#[test]
fn known_enum_variants_deserialize() {
    let json =
        br#"{"id":"1","name":"Widget","category":"electronics","condition":"new","notes":"test"}"#
            .to_vec();
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: json,
    };
    let decoded: GetItemResponse =
        operations::get_item::decode_get_item_response(response.as_bytes())
            .expect("decoded response");
    match decoded {
        GetItemResponse::Ok(item) => {
            assert_eq!(item.id, "1");
            assert_eq!(item.name, "Widget");
            assert_eq!(CATEGORY, "electronics");
            assert_eq!(item.category, ItemCategory::Electronics);
            assert_eq!(item.condition, ItemCondition::New);
            assert_eq!(item.notes, Some("test".to_owned()));
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn unknown_closed_enum_variant_is_rejected() {
    let json = br#"{"id":"2","name":"Gadget","category":"unknown_category","condition":"new","notes":null}"#.to_vec();
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: json,
    };
    assert!(operations::get_item::decode_get_item_response::<satay_runtime::storage::AllocStorage>(response.as_bytes()).is_err());
}
