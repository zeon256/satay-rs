use std::collections::BTreeSet;

use heck::{ToSnakeCase, ToUpperCamelCase};

pub(crate) fn type_ident(value: &str) -> String {
    let ident = value.to_upper_camel_case();
    sanitize_ident(&ident, "GeneratedType", IdentKind::Type)
}

pub(crate) fn variant_ident(value: &str) -> String {
    let ident = value.to_upper_camel_case();
    sanitize_ident(&ident, "Value", IdentKind::Type)
}

pub(crate) fn response_variant_ident(status: u16) -> String {
    match status {
        200 => "Ok".to_owned(),
        201 => "Created".to_owned(),
        202 => "Accepted".to_owned(),
        204 => "NoContent".to_owned(),
        400 => "BadRequest".to_owned(),
        401 => "Unauthorized".to_owned(),
        403 => "Forbidden".to_owned(),
        404 => "NotFound".to_owned(),
        409 => "Conflict".to_owned(),
        422 => "UnprocessableEntity".to_owned(),
        500 => "InternalServerError".to_owned(),
        _ => format!("Status{status}"),
    }
}

pub(crate) fn field_ident(value: &str) -> String {
    let ident = value.to_snake_case();
    sanitize_ident(&ident, "field", IdentKind::Value)
}

pub(crate) fn function_ident(value: &str) -> String {
    let ident = value.to_snake_case();
    sanitize_ident(&ident, "operation", IdentKind::Value)
}

#[derive(Debug, Clone, Copy)]
enum IdentKind {
    Type,
    Value,
}

fn sanitize_ident(value: &str, fallback: &str, kind: IdentKind) -> String {
    let mut ident = String::with_capacity(value.len().max(fallback.len()));
    for ch in value.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            ident.push(ch);
        } else if !ident.ends_with('_') {
            ident.push('_');
        }
    }

    while ident.starts_with('_') {
        ident.remove(0);
    }
    while ident.ends_with('_') {
        ident.pop();
    }
    if ident.is_empty() {
        ident.push_str(fallback);
    }

    let starts_with_digit = ident
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_digit());
    if starts_with_digit {
        match kind {
            IdentKind::Type => ident.insert_str(0, fallback),
            IdentKind::Value => ident.insert(0, '_'),
        }
    }

    if is_rust_keyword(&ident) {
        ident.push('_');
    }
    ident
}

pub(crate) fn unique_ident(candidate: String, used: &mut BTreeSet<String>) -> String {
    if used.insert(candidate.clone()) {
        return candidate;
    }

    for suffix in 2.. {
        let next = format!("{candidate}_{suffix}");
        if used.insert(next.clone()) {
            return next;
        }
    }
    unreachable!()
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_rust_keywords_for_types_and_values() {
        assert_eq!(type_ident("type"), "Type");
        assert_eq!(field_ident("type"), "type_");
        assert_eq!(function_ident("async"), "async_");
        assert_eq!(variant_ident("Self"), "Self_");
    }

    #[test]
    fn prefixes_identifiers_that_start_with_digits() {
        assert_eq!(type_ident("123 status"), "GeneratedType123Status");
        assert_eq!(field_ident("123 status"), "_123_status");
        assert_eq!(function_ident("404"), "_404");
    }

    #[test]
    fn replaces_invalid_characters_and_uses_fallback_for_empty_names() {
        assert_eq!(field_ident("$skip"), "skip");
        assert_eq!(field_ident("user/id"), "user_id");
        assert_eq!(field_ident(" user--id "), "user_id");
        assert_eq!(field_ident(""), "field");
        assert_eq!(type_ident(""), "GeneratedType");
        assert_eq!(variant_ident(""), "Value");
    }

    #[test]
    fn allocates_stable_duplicate_suffixes() {
        let mut used = BTreeSet::new();

        assert_eq!(unique_ident("body".to_owned(), &mut used), "body");
        assert_eq!(unique_ident("body".to_owned(), &mut used), "body_2");
        assert_eq!(unique_ident("body".to_owned(), &mut used), "body_3");
    }
}
