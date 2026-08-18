use std::collections::{BTreeMap, BTreeSet};

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
    let std_use_count = items.len();

    let mut runtime_serde_imports = BTreeSet::new();
    for component in &api.components {
        render_component(component, &mut items, &mut runtime_serde_imports);
    }
    for constrained_type in &api.constrained_types {
        items.push(Item::Struct(constrained::render_constrained_type(
            constrained_type,
        )));
    }

    for (offset, import) in serde_use_items(&runtime_serde_imports)
        .into_iter()
        .enumerate()
    {
        items.insert(std_use_count + offset, import);
    }

    syn::File {
        shebang: None,
        attrs: vec![],
        items,
    }
}

/// Renders the collected runtime serde module imports as cfg-gated `use`
/// items so call sites stay short enough for the `minimal_imports` lint.
/// Aliased `option` submodules emit standalone imports; plain leaves are
/// grouped per runtime parent module.
fn serde_use_items(imports: &BTreeSet<String>) -> Vec<Item> {
    let mut plain = BTreeMap::<&str, BTreeSet<&str>>::new();
    let mut aliased = vec![];
    for import in imports {
        if let Some((path, alias)) = import.split_once(" as ") {
            aliased.push(format!(
                "#[cfg(feature = \"serde\")] use {path} as {alias};"
            ));
        } else if let Some((parent, leaf)) = import.rsplit_once("::") {
            plain.entry(parent).or_default().insert(leaf);
        } else {
            unreachable!("runtime serde imports are multi-segment paths");
        }
    }

    aliased.sort();
    let mut use_items = aliased;
    for (parent, leaves) in &plain {
        let import = if leaves.len() == 1 {
            format!(
                "#[cfg(feature = \"serde\")] use {parent}::{};",
                leaves.iter().next().expect("single leaf")
            )
        } else {
            let leaves = leaves.iter().copied().collect::<Vec<_>>().join(", ");
            format!("#[cfg(feature = \"serde\")] use {parent}::{{{leaves}}};")
        };
        use_items.push(import);
    }

    use_items
        .into_iter()
        .map(|import| syn::parse_str(&import).expect("runtime serde import path is valid"))
        .collect()
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

fn render_component(
    component: &Component,
    items: &mut Vec<syn::Item>,
    runtime_serde_imports: &mut BTreeSet<String>,
) {
    match &component.kind {
        ComponentKind::Struct(fields) => {
            items.push(Item::Struct(structs::render_struct(
                &component.rust_name,
                component.description.as_deref(),
                fields,
                true,
            )));
            if let Some(impl_) = structs::render_field_serde_impl(
                &component.rust_name,
                fields,
                runtime_serde_imports,
            ) {
                items.push(Item::Impl(impl_));
            }
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
