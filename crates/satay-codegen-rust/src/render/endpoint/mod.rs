use syn::{Ident, Item, parse_quote};

use crate::model::{Api, Operation, TypeRef};

use super::family::Family;

mod input;

pub(super) use input::has_text_default;
mod json;
pub(super) mod parts;
mod response;

pub(super) fn render_endpoint_mod(_operation: &Operation) -> syn::File {
    syn::File {
        shebang: None,
        attrs: vec![],
        items: vec![
            parse_quote!(
                mod parts;
            ),
            parse_quote!(
                #[cfg(feature = "json")]
                mod json;
            ),
            parse_quote!(
                pub use parts::*;
            ),
            parse_quote!(
                #[cfg(feature = "json")]
                pub use json::*;
            ),
        ],
    }
}

pub(super) fn render_endpoint_parts_file(
    api: &Api,
    family: &Family,
    operation: &Operation,
) -> syn::File {
    let mut items = vec![];
    if let Some(use_types) = build_parts_types_use(api, operation) {
        items.push(Item::Use(use_types));
    }
    items.extend(input::render_input(family, operation));
    items.push(Item::Impl(input::render_input_impl(family, operation)));
    if let Some(default_impl) = input::render_input_default_impl(family, operation) {
        items.push(Item::Impl(default_impl));
    }
    items.extend(response::render_response(family, operation));
    items.push(Item::Fn(parts::render_parts_function(family, operation)));

    syn::File {
        shebang: None,
        attrs: vec![],
        items,
    }
}

pub(super) fn render_endpoint_json_file(
    api: &Api,
    family: &Family,
    operation: &Operation,
) -> syn::File {
    let mut items = vec![];
    if let Some(use_types) = build_json_types_use(api, family, operation) {
        items.push(Item::Use(use_types));
    }
    let input_name = super::ident(&operation.input_name);
    let response_name = super::ident(&operation.response_name);
    let parts_fn = super::ident(&format!("{}_parts", operation.fn_name));
    items.push(Item::Use(
        parse_quote!(use super::parts::{#input_name, #response_name, #parts_fn};),
    ));
    items.push(Item::Fn(json::render_encode_function(family, operation)));
    items.extend(
        json::render_decode_functions(family, operation)
            .into_iter()
            .map(Item::Fn),
    );

    syn::File {
        shebang: None,
        attrs: vec![],
        items,
    }
}

fn build_parts_types_use(api: &Api, operation: &Operation) -> Option<syn::ItemUse> {
    let mut needed_names: Vec<Ident> = vec![];

    for param in &operation.parameters {
        collect_type_refs(&param.ty, &mut needed_names);
    }
    if let Some(body) = &operation.request_body {
        collect_type_refs(&body.ty, &mut needed_names);
    }
    for response in &operation.responses {
        if let Some(body) = &response.body {
            collect_type_refs(body, &mut needed_names);
        }
    }

    build_types_use(api, needed_names)
}

fn build_json_types_use(api: &Api, family: &Family, operation: &Operation) -> Option<syn::ItemUse> {
    let mut needed_names: Vec<Ident> = vec![];

    for response in &operation.responses {
        if let Some(body) = &response.body {
            collect_decoded_type_refs(family, body, &mut needed_names);
        }
    }

    build_types_use(api, needed_names)
}

/// Collects the names a decoder spells. Aliases of family projections decode
/// through their target's shape, so the alias itself is never named.
fn collect_decoded_type_refs(family: &Family, ty: &TypeRef, names: &mut Vec<Ident>) {
    match ty {
        TypeRef::Named(name) => match family.projected_alias(name) {
            Some(target) => collect_decoded_type_refs(family, target, names),
            None => names.push(super::ident(name)),
        },
        TypeRef::Array(inner) | TypeRef::Map(inner) | TypeRef::Option(inner) => {
            collect_decoded_type_refs(family, inner, names);
        }
        other => collect_type_refs(other, names),
    }
}

fn build_types_use(api: &Api, mut all_names: Vec<Ident>) -> Option<syn::ItemUse> {
    use std::collections::BTreeSet;

    all_names.sort_by_key(|a| a.to_string());
    all_names.dedup();

    // Filter: only import names that are in the types module (components and constrained types)
    let type_names = api
        .components
        .iter()
        .map(|c| c.rust_name.clone())
        .chain(api.constrained_types.iter().map(|c| c.rust_name.clone()))
        .collect::<BTreeSet<String>>();

    let names = all_names
        .into_iter()
        .filter(|n| type_names.contains(&n.to_string()))
        .collect::<Vec<Ident>>();

    (!names.is_empty()).then(|| parse_quote!(use super::super::types::{#(#names),*};))
}

fn collect_type_refs(ty: &TypeRef, names: &mut Vec<Ident>) {
    use TypeRef;
    match ty {
        TypeRef::Named(name) => {
            names.push(super::ident(name));
        }
        TypeRef::Coordinates(codec) => {
            names.push(super::ident(codec.target()));
        }
        TypeRef::Constrained { rust_name, .. } => {
            names.push(super::ident(rust_name));
        }
        TypeRef::Array(inner) => collect_type_refs(inner, names),
        TypeRef::Map(inner) => collect_type_refs(inner, names),
        TypeRef::Option(inner) => collect_type_refs(inner, names),
        TypeRef::Range(range_type) => {
            names.push(super::ident(&range_type.rust_name));
        }
        TypeRef::String
        | TypeRef::ParsedString(_)
        | TypeRef::ParsedInteger(_)
        | TypeRef::Integer(_)
        | TypeRef::F32
        | TypeRef::F64
        | TypeRef::Bool
        | TypeRef::JsonValue => {}
    }
}
