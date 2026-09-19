use super::generated::*;
use satay_runtime::Url;

#[test]
fn url_fields_round_trip() {
    let json = serde_json::json!({
        "url": "https://example.com/path?q=1#fragment",
        "nullable": null,
        "urls": ["mailto:hello@example.com"],
        "byName": {"home": "https://example.com/"},
        "lenient": "invalid",
        "sentinel": ""
    });
    let links: Links = serde_json::from_value(json.clone()).unwrap();
    let _: &Url = &links.url;
    assert_eq!(links.url.host_str(), Some("example.com"));
    assert!(links.nullable.is_none());
    assert!(links.optional.is_none());
    assert!(links.lenient.is_none());
    assert!(links.sentinel.is_none());
    assert_eq!(links.urls[0].scheme(), "mailto");
    assert_eq!(links.by_name["home"].path(), "/");
    let encoded = serde_json::to_value(&links).unwrap();
    assert_eq!(encoded["url"], json["url"]);
    assert_eq!(encoded["urls"], json["urls"]);
    assert_eq!(encoded["byName"], json["byName"]);
    for invalid in [serde_json::json!("/relative"), serde_json::json!(42)] {
        let mut bad = json.clone();
        bad["url"] = invalid;
        assert!(serde_json::from_value::<Links>(bad).is_err());
    }
    let mut present = json;
    present["optional"] = serde_json::json!("https://example.com/optional");
    present["nullable"] = serde_json::json!("https://example.com/nullable");
    let links: Links = serde_json::from_value(present).unwrap();
    assert_eq!(links.optional.unwrap().path(), "/optional");
    assert_eq!(links.nullable.unwrap().path(), "/nullable");
}

#[test]
fn urls_are_encoded_as_query_values() {
    let url = Url::parse("https://example.com/path?a=1&b=2#fragment").unwrap();
    let request = Api::new()
        .base_url("https://api.example.com")
        .untagged()
        .get_links(url)
        .request()
        .unwrap();
    assert_eq!(
        request.uri().query(),
        Some("target=https%3A%2F%2Fexample.com%2Fpath%3Fa%3D1%26b%3D2%23fragment")
    );
}
