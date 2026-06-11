//! Assertion helpers that parse generated file contents back into a `syn` AST,
//! so tests check structure instead of `prettyplease` formatting artifacts.

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use satay_codegen::GeneratedFile;
use syn::{Expr, ExprLit, Fields, ImplItem, Item, Lit, Meta, MetaNameValue, Type, Visibility};

pub fn parse_rust(file: &GeneratedFile) -> syn::File {
    syn::parse_file(&file.contents).unwrap_or_else(|err| {
        panic!(
            "failed to parse generated file {}: {err}",
            file.relative_path
        )
    })
}

/// Normalized token string; equal for code that differs only in formatting.
/// Prints token-by-token so punctuation spacing hints (`::*` vs `:: *`) never
/// make equal token sequences compare unequal.
pub fn norm(tokens: &impl ToTokens) -> String {
    canon(tokens.to_token_stream())
}

pub fn norm_str(fragment: &str) -> String {
    canon(
        fragment
            .parse::<TokenStream>()
            .unwrap_or_else(|err| panic!("failed to tokenize fragment `{fragment}`: {err}")),
    )
}

fn canon(stream: TokenStream) -> String {
    fn push(stream: TokenStream, out: &mut String) {
        for tree in stream {
            match tree {
                TokenTree::Group(group) => {
                    let (open, close) = match group.delimiter() {
                        Delimiter::Parenthesis => ("(", ")"),
                        Delimiter::Brace => ("{", "}"),
                        Delimiter::Bracket => ("[", "]"),
                        Delimiter::None => ("", ""),
                    };
                    out.push_str(open);
                    push(group.stream(), out);
                    out.push_str(close);
                }
                other => out.push_str(&other.to_string()),
            }
            out.push(' ');
        }
        if out.ends_with(' ') {
            out.pop();
        }
    }
    let mut out = String::new();
    push(stream, &mut out);
    out
}

pub fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

macro_rules! finder {
    ($find:ident, $variant:ident, $ty:ty, $kind:literal) => {
        pub fn $find<'a>(file: &'a syn::File, name: &str) -> &'a $ty {
            file.items
                .iter()
                .find_map(|item| match item {
                    Item::$variant(inner) if inner.ident == name => Some(inner),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let available = file
                        .items
                        .iter()
                        .filter_map(|item| match item {
                            Item::$variant(inner) => Some(inner.ident.to_string()),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    panic!(
                        "no {} `{name}` in generated file; available: {available:?}",
                        $kind
                    )
                })
        }
    };
}

finder!(find_struct, Struct, syn::ItemStruct, "struct");
finder!(find_enum, Enum, syn::ItemEnum, "enum");
finder!(find_const, Const, syn::ItemConst, "const");
finder!(find_type_alias, Type, syn::ItemType, "type alias");
finder!(find_mod, Mod, syn::ItemMod, "module");

pub fn find_fn<'a>(file: &'a syn::File, name: &str) -> &'a syn::ItemFn {
    file.items
        .iter()
        .find_map(|item| match item {
            Item::Fn(inner) if inner.sig.ident == name => Some(inner),
            _ => None,
        })
        .unwrap_or_else(|| {
            let available = file
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Fn(inner) => Some(inner.sig.ident.to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            panic!("no fn `{name}` in generated file; available: {available:?}")
        })
}

pub fn has_enum(file: &syn::File, name: &str) -> bool {
    file.items
        .iter()
        .any(|item| matches!(item, Item::Enum(inner) if inner.ident == name))
}

fn type_base_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(reference) => type_base_name(&reference.elem),
        _ => None,
    }
}

/// Finds a method in an inherent `impl` block, ignoring generics/lifetimes on
/// the self type (`impl GetUserAction<'_>` matches self_ty `"GetUserAction"`).
pub fn find_method<'a>(file: &'a syn::File, self_ty: &str, name: &str) -> &'a syn::ImplItemFn {
    file.items
        .iter()
        .filter_map(|item| match item {
            Item::Impl(imp) if imp.trait_.is_none() => Some(imp),
            _ => None,
        })
        .filter(|imp| type_base_name(&imp.self_ty).as_deref() == Some(self_ty))
        .find_map(|imp| {
            imp.items.iter().find_map(|item| match item {
                ImplItem::Fn(method) if method.sig.ident == name => Some(method),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("no inherent method `{self_ty}::{name}` in generated file"))
}

pub fn field<'a>(item: &'a syn::ItemStruct, name: &str) -> &'a syn::Field {
    item.fields
        .iter()
        .find(|f| f.ident.as_ref().is_some_and(|ident| ident == name))
        .unwrap_or_else(|| {
            panic!(
                "struct `{}` has no field `{name}`; fields: {:?}",
                item.ident,
                field_names(item)
            )
        })
}

pub fn field_names(item: &syn::ItemStruct) -> Vec<String> {
    item.fields
        .iter()
        .filter_map(|f| f.ident.as_ref().map(ToString::to_string))
        .collect()
}

pub fn assert_field(item: &syn::ItemStruct, name: &str, ty: &str) {
    let field = field(item, name);
    assert!(
        is_pub(&field.vis),
        "field `{}.{name}` is not pub",
        item.ident
    );
    assert_eq!(
        norm(&field.ty),
        norm_str(ty),
        "type of field `{}.{name}`",
        item.ident
    );
}

pub fn assert_tuple_struct(file: &syn::File, name: &str, inner_ty: &str) {
    let item = find_struct(file, name);
    match &item.fields {
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            assert_eq!(
                norm(&fields.unnamed[0].ty),
                norm_str(inner_ty),
                "inner type of tuple struct `{name}`"
            );
        }
        other => panic!(
            "expected `{name}` to be a single-field tuple struct, got fields `{}`",
            norm(other)
        ),
    }
}

pub fn variant<'a>(item: &'a syn::ItemEnum, name: &str) -> &'a syn::Variant {
    item.variants
        .iter()
        .find(|v| v.ident == name)
        .unwrap_or_else(|| {
            panic!(
                "enum `{}` has no variant `{name}`; variants: {:?}",
                item.ident,
                variant_names(item)
            )
        })
}

pub fn variant_names(item: &syn::ItemEnum) -> Vec<String> {
    item.variants.iter().map(|v| v.ident.to_string()).collect()
}

pub fn doc_lines(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| match &attr.meta {
            Meta::NameValue(MetaNameValue {
                value:
                    Expr::Lit(ExprLit {
                        lit: Lit::Str(text),
                        ..
                    }),
                ..
            }) => Some(text.value().trim().to_owned()),
            _ => None,
        })
        .collect()
}

pub fn assert_doc(attrs: &[syn::Attribute], expected: &str) {
    let docs = doc_lines(attrs);
    assert!(
        docs.iter().any(|line| line == expected),
        "doc line `{expected}` not found; docs: {docs:?}"
    );
}

pub fn has_cfg_feature(attrs: &[syn::Attribute], feature: &str) -> bool {
    let expected = norm_str(&format!(r#"#[cfg(feature = "{feature}")]"#));
    attrs.iter().any(|attr| norm(attr) == expected)
}

/// Asserts some attribute with the given path (e.g. `cfg_attr`, `nutype::nutype`)
/// contains `fragment`, compared as normalized tokens.
pub fn assert_attr_contains(attrs: &[syn::Attribute], path: &str, fragment: &str) {
    let path_norm = norm_str(path);
    let fragment_norm = norm_str(fragment);
    let matching = attrs
        .iter()
        .filter(|attr| norm(attr.path()) == path_norm)
        .map(norm)
        .collect::<Vec<_>>();
    assert!(
        matching
            .iter()
            .any(|tokens| tokens.contains(&fragment_norm)),
        "no `{path}` attribute containing `{fragment}`; matching attributes: {matching:?}"
    );
}

/// Token-level fallback for code inside function bodies or whole-file checks.
pub fn contains_tokens(item: &impl ToTokens, fragment: &str) -> bool {
    norm(item).contains(&norm_str(fragment))
}

/// Stricter than a text search: matches identifiers only, not string literals
/// or substrings of longer names.
pub fn contains_ident(file: &syn::File, ident: &str) -> bool {
    fn walk(stream: TokenStream, ident: &str) -> bool {
        stream.into_iter().any(|tree| match tree {
            TokenTree::Ident(found) => found == ident,
            TokenTree::Group(group) => walk(group.stream(), ident),
            _ => false,
        })
    }
    walk(file.to_token_stream(), ident)
}
