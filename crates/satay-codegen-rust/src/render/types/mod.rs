use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Api, Component, ComponentKind};
use syn::Item;

use super::family::{Family, Spelling};

mod constrained;
mod enums;
mod ranges;
pub(super) mod structs;
mod unions;

pub(super) fn render_types_file(api: &Api, family: &Family) -> syn::File {
    let mut items = vec![];
    let mut runtime_serde_imports = BTreeSet::new();
    for component in &api.components {
        render_component(family, component, &mut items, &mut runtime_serde_imports);
    }
    for constrained_type in &api.constrained_types {
        items.push(Item::Struct(constrained::render_constrained_type(
            family,
            constrained_type,
        )));
    }

    items.splice(0..0, serde_use_items(&runtime_serde_imports));

    syn::File {
        shebang: None,
        attrs: vec![],
        items,
    }
}

/// Renders the collected runtime serde module imports as cfg-gated `use`
/// items so call sites and attribute strings stay short enough for the
/// `minimal_imports` lint. Aliased `option` submodules emit standalone
/// imports; plain leaves are grouped per runtime parent module.
fn serde_use_items(imports: &BTreeSet<String>) -> Vec<Item> {
    fn cfg_attr(import: &str) -> &'static str {
        // `treat_error_as_none` is `json`-gated in satay-runtime; every other
        // serde module is `serde`-gated.
        if import == "satay_runtime::treat_error_as_none" {
            r#"#[cfg(all(feature = "serde", feature = "json"))]"#
        } else {
            r#"#[cfg(feature = "serde")]"#
        }
    }

    let mut use_items = vec![];
    let mut aliased = vec![];
    let mut plain = BTreeMap::<&str, BTreeSet<&str>>::new();
    for import in imports {
        if let Some((path, alias)) = import.split_once(" as ") {
            aliased.push(format!(r"{} use {path} as {alias};", cfg_attr(import)));
        } else if *import == "satay_runtime::treat_error_as_none" {
            // Never grouped with `serde`-gated siblings: its cfg differs.
            use_items.push(format!(r"{} use {import};", cfg_attr(import)));
        } else if let Some((parent, leaf)) = import.rsplit_once("::") {
            plain.entry(parent).or_default().insert(leaf);
        } else {
            unreachable!("runtime serde imports are multi-segment paths");
        }
    }

    aliased.sort();
    use_items.extend(aliased);
    for (parent, leaves) in &plain {
        let import = if leaves.len() == 1 {
            format!(
                r#"#[cfg(feature = "serde")] use {parent}::{};"#,
                leaves.iter().next().expect("single leaf")
            )
        } else {
            let leaves = leaves.iter().copied().collect::<Vec<_>>().join(", ");
            format!(r#"#[cfg(feature = "serde")] use {parent}::{{{leaves}}};"#)
        };
        use_items.push(import);
    }

    use_items
        .into_iter()
        .map(|import| syn::parse_str(&import).expect("runtime serde import path is valid"))
        .collect()
}

fn render_component(
    family: &Family,
    component: &Component,
    items: &mut Vec<syn::Item>,
    runtime_serde_imports: &mut BTreeSet<String>,
) {
    match &component.kind {
        ComponentKind::Struct(fields) => items.extend(structs::render_struct(
            family,
            &component.rust_name,
            component.description.as_deref(),
            fields,
            true,
            runtime_serde_imports,
        )),
        ComponentKind::Enum(enum_) => items.extend(enums::render_enum(
            family,
            &component.rust_name,
            component.description.as_deref(),
            enum_,
        )),
        ComponentKind::Union(union) => items.extend(unions::render_union(
            family,
            &component.rust_name,
            component.description.as_deref(),
            union,
        )),
        ComponentKind::Range(range_type) => items.extend(ranges::render_range_type(range_type)),
        ComponentKind::Alias(ty) => {
            let name = super::ident(&component.rust_name);
            let docs = super::doc_attrs(component.description.as_deref());
            if family.is_generic(&component.rust_name) {
                // Aliases cannot bound their parameters, so the projection names
                // its trait explicitly.
                let storage = family.storage();
                let ty = family.ty(ty, Spelling::Qualified);
                items.push(syn::parse_quote!(
                    #(#docs)*
                    pub type #name<'s, #storage = satay_runtime::AllocStorage> = #ty;
                ));
            } else {
                let ty = family.ty(ty, Spelling::Concrete);
                items.push(syn::parse_quote!(#(#docs)* pub type #name = #ty;));
            }
        }
        ComponentKind::Nutype(constrained_type) => {
            items.push(Item::Struct(constrained::render_constrained_type(
                family,
                constrained_type,
            )));
        }
    }
}
