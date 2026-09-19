use super::*;

#[test]
fn filters_blank_descriptions() {
    assert_eq!(optional_description(&None), None);
    assert_eq!(optional_description(&Some(String::new())), None);
    assert_eq!(optional_description(&Some(" \n\t ".to_owned())), None);
    assert_eq!(
        optional_description(&Some("  useful text  ".to_owned())),
        Some("  useful text  ".to_owned())
    );
}

#[test]
fn matches_json_media_types_case_insensitively() {
    assert!(is_json_media_type("application/json"));
    assert!(is_json_media_type("Application/JSON; charset=utf-8"));
    assert!(is_json_media_type("application/vnd.satay.user+json"));
    assert!(is_json_media_type("application/problem+JSON"));
    assert!(!is_json_media_type("text/json"));
    assert!(!is_json_media_type("application/xml"));
    assert!(!is_json_media_type("not-a-media-type"));
}

#[test]
fn selects_explicit_json_before_suffix_json_media_type() {
    let mut content = OasMap::new();
    content.insert(
        "application/vnd.satay.user+json".to_owned(),
        OasMediaType::default(),
    );
    content.insert("application/json".to_owned(), OasMediaType::default());

    let (media_type, _) = json_media_type(&content).expect("json media type");
    assert_eq!(media_type, "application/json");
}

#[test]
fn selects_first_suffix_json_media_type_when_exact_json_is_absent() {
    let mut content = OasMap::new();
    content.insert("application/xml".to_owned(), OasMediaType::default());
    content.insert(
        "application/vnd.satay.user+json; charset=utf-8".to_owned(),
        OasMediaType::default(),
    );

    let (media_type, _) = json_media_type(&content).expect("json media type");
    assert_eq!(media_type, "application/vnd.satay.user+json; charset=utf-8");
}
