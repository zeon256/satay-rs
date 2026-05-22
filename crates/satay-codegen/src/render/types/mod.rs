use crate::model::{Api, Component, ComponentKind};
use syn::Item;

mod constrained;
mod enums;
mod ranges;
pub(super) mod structs;

pub(super) fn render_types_file(api: &Api) -> syn::File {
    let mut items = Vec::new();
    let has_enum = api
        .components
        .iter()
        .any(|component| matches!(component.kind, ComponentKind::Enum(_)));
    let has_range = api
        .components
        .iter()
        .any(|component| matches!(component.kind, ComponentKind::Range(_)));
    if has_range {
        items.push(syn::parse_quote!(
            use std::{convert, fmt};
        ));
    } else if has_enum {
        items.push(syn::parse_quote!(
            use std::fmt;
        ));
    }
    for component in &api.components {
        render_component(component, &mut items);
    }
    for constrained_type in &api.constrained_types {
        items.push(Item::Struct(constrained::render_constrained_type(
            constrained_type,
        )));
    }

    syn::File {
        shebang: None,
        attrs: vec![],
        items,
    }
}

fn render_component(component: &Component, items: &mut Vec<syn::Item>) {
    match &component.kind {
        ComponentKind::Struct(fields) => {
            items.push(Item::Struct(structs::render_struct(
                &component.rust_name,
                component.description.as_deref(),
                fields,
                true,
            )));
        }
        ComponentKind::Enum(variants) => items.extend(enums::render_enum(
            &component.rust_name,
            component.description.as_deref(),
            variants,
        )),
        ComponentKind::Range(range_type) => items.extend(ranges::render_range_type(range_type)),
        ComponentKind::Alias(ty) => {
            let name = super::ident(&component.rust_name);
            let ty = super::rust_type(ty);
            let docs = super::doc_attrs(component.description.as_deref());
            items.push(syn::parse_quote!(#(#docs)* pub type #name = #ty;));
        }
        ComponentKind::Nutype(constrained_type) => {
            items.push(Item::Struct(constrained::render_constrained_type(
                constrained_type,
            )));
        }
    }
}
