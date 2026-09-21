use super::*;

#[test]
fn escapes_rust_keywords_for_types_and_values() {
    assert_eq!(type_ident("type"), "Type");
    assert_eq!(field_ident("type"), "r#type");
    assert_eq!(field_ident("self"), "self_");
    assert_eq!(function_ident("async"), "async_");
    assert_eq!(group_ident("Bus Service"), "bus_service");
    assert_eq!(variant_ident("Self"), "Self_");
}

#[test]
fn recognizes_rust_keywords() {
    for keyword in [
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
        "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ] {
        assert!(
            is_rust_keyword(keyword),
            "expected {keyword} to be a keyword"
        );
    }

    for non_keyword in ["Type", "self_", "async_", "satay"] {
        assert!(
            !is_rust_keyword(non_keyword),
            "expected {non_keyword} not to be a keyword"
        );
    }
}

#[test]
fn prefixes_identifiers_that_start_with_digits() {
    assert_eq!(type_ident("123 status"), "GeneratedType123Status");
    assert_eq!(field_ident("123 status"), "_123_status");
    assert_eq!(function_ident("404"), "_404");
    assert_eq!(group_ident("404"), "_404");
}

#[test]
fn replaces_invalid_characters_and_uses_fallback_for_empty_names() {
    assert_eq!(field_ident("$skip"), "skip");
    assert_eq!(field_ident("user/id"), "user_id");
    assert_eq!(field_ident(" user--id "), "user_id");
    assert_eq!(field_ident(""), "field");
    assert_eq!(type_ident(""), "GeneratedType");
    assert_eq!(group_ident(""), "group");
    assert_eq!(variant_ident(""), "Value");
}

#[test]
fn allocates_stable_duplicate_suffixes() {
    let mut used = BTreeSet::new();

    assert_eq!(unique_ident("body".to_owned(), &mut used), "body");
    assert_eq!(unique_ident("body".to_owned(), &mut used), "body_2");
    assert_eq!(unique_ident("body".to_owned(), &mut used), "body_3");
}
