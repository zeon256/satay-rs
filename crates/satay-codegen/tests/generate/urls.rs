use std::fs;

use crate::common::*;

#[test]
fn uri_format_round_trips_and_builds_requests() {
    let spec = r#"
openapi: 3.1.0
info:
  title: URL API
  version: 1.0.0
paths:
  /links:
    get:
      operationId: getLinks
      parameters:
        - name: target
          in: query
          required: true
          schema:
            type: string
            format: uri
      responses:
        '204':
          description: No content
components:
  schemas:
    Link:
      type: string
      format: uri
    Links:
      type: object
      required: [url, nullable, urls, byName]
      properties:
        url:
          $ref: '#/components/schemas/Link'
        nullable:
          type: [string, 'null']
          format: uri
        optional:
          type: string
          format: uri
        lenient:
          type: string
          format: uri
          x-satay:
            treat-error-as-none: true
        sentinel:
          type: string
          format: uri
          x-satay:
            none-if: ['']
        urls:
          type: array
          items:
            type: string
            format: uri
        byName:
          type: object
          additionalProperties:
            type: string
            format: uri
"#;
    let files = satay_codegen::generate(spec).expect("generate URL fixture");
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    write_manifest(dir, &runtime_path_toml(), false, false);
    write_generated_files(&dir.join("src/generated"), &files);
    fs::write(dir.join("src/lib.rs"), r##"
pub mod generated;

#[cfg(test)]
mod tests {
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
        let request = Api::new().base_url("https://api.example.com")
            .untagged().get_links(url).request().unwrap();
        assert_eq!(request.uri().query(), Some("target=https%3A%2F%2Fexample.com%2Fpath%3Fa%3D1%26b%3D2%23fragment"));
    }
}
"##).unwrap();
    run_temp_cargo(dir, "test", &[], "generated URL behavior");
    run_temp_cargo(
        dir,
        "check",
        &["--no-default-features"],
        "URLs without serde",
    );
    // Lossy decoding requires JSON; check serde-only support without that option.
    let serde_spec = spec.replace(
        "          x-satay:\n            treat-error-as-none: true\n",
        "",
    );
    let files = satay_codegen::generate(&serde_spec).unwrap();
    write_generated_files(&dir.join("src/generated"), &files);
    run_temp_cargo(
        dir,
        "check",
        &["--no-default-features", "--features", "serde"],
        "URLs with serde only",
    );
}
