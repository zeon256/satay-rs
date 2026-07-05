use crate::model::{Api, Component, ComponentKind};
use syn::Item;

mod constrained;
mod enums;
mod ranges;
pub(super) mod structs;
mod unions;

pub(super) fn render_types_file(api: &Api) -> syn::File {
    let mut items = vec![];
    let has_enum = api
        .components
        .iter()
        .any(|component| matches!(component.kind, ComponentKind::Enum(_)));
    let has_range = api
        .components
        .iter()
        .any(|component| matches!(component.kind, ComponentKind::Range(_)));
    let has_map = api.components.iter().any(component_contains_map)
        || api
            .constrained_types
            .iter()
            .any(|constrained| constrained.inner.contains_map());
    if has_map {
        items.push(syn::parse_quote!(
            use std::collections::BTreeMap;
        ));
    }
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

fn component_contains_map(component: &Component) -> bool {
    match &component.kind {
        ComponentKind::Struct(fields) => fields.iter().any(|field| field.ty.contains_map()),
        ComponentKind::Union(union) => union
            .variants
            .iter()
            .any(|variant| variant.ty.contains_map()),
        ComponentKind::Alias(ty) => ty.contains_map(),
        ComponentKind::Nutype(constrained) => constrained.inner.contains_map(),
        ComponentKind::Enum(_) | ComponentKind::Range(_) => false,
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
        ComponentKind::Enum(enum_) => items.extend(enums::render_enum(
            &component.rust_name,
            component.description.as_deref(),
            enum_,
        )),
        ComponentKind::Union(union) => {
            items.push(Item::Enum(unions::render_union(
                &component.rust_name,
                component.description.as_deref(),
                union,
            )));
        }
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
