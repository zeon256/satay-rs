use super::*;
use serde_json::Number;

#[test]
fn json_media_type_classification_matches_frontend_behavior() {
    assert!(is_json_media_type("application/json"));
    assert!(is_json_media_type("application/json; charset=utf-8"));
    assert!(is_json_media_type("application/vnd.api+json"));
    assert!(!is_json_media_type("text/plain"));
    assert!(!is_json_media_type("application/octet-stream"));
    assert!(!is_json_media_type("json"));
}

#[test]
fn json_integer_widens_integral_values_and_rejects_others() {
    assert_eq!(json_integer(&Number::from(5), "ctx").unwrap(), 5);
    assert_eq!(
        json_integer(&Number::from(u64::MAX), "ctx").unwrap(),
        i128::from(u64::MAX)
    );
    assert_eq!(
        json_integer(&Number::from_f64(2.0).unwrap(), "ctx").unwrap(),
        2
    );
    let error = json_integer(&Number::from_f64(2.5).unwrap(), "ctx").unwrap_err();
    assert!(matches!(error, ValidationError::ExpectedInteger { .. }));
}

#[test]
fn reject_keyword_passes_absent_keywords_and_rejects_present_ones() {
    assert!(reject_keyword(false, "pattern", "ctx").is_ok());
    let error = reject_keyword(true, "pattern", "ctx").unwrap_err();
    assert!(matches!(
        error,
        ValidationError::UnsupportedKeyword { keyword, .. } if keyword == "pattern"
    ));
}

#[test]
fn property_context_formats_property_paths() {
    assert_eq!(
        property_context("schema `User`", "age"),
        "property `User.age`"
    );
    assert_eq!(
        property_context("operation `list`", "age"),
        "property `operation `list`.age`"
    );
}

#[test]
fn inferred_operation_id_tracks_path_parameters() {
    assert_eq!(
        inferred_operation_id("get", "/users/{id}/posts/{postId}"),
        "get_users_by_id_posts_by_postId"
    );
    assert_eq!(inferred_operation_id("get", "/"), "get");
}
