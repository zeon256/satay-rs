//! Adds the `use` items that a rendered file's short paths rely on.
//!
//! Renderers write `fmt::Debug`, `shape::Text`, or `de::Visitor` so generated
//! paths stay at two segments. This pass finds which roots a file mentions and
//! imports exactly those, gating an import when every use is `serde`-gated.

use quote::ToTokens;
use syn::Item;
use syn::visit_mut::{self, VisitMut};

#[derive(Clone, Copy, Default)]
struct Usage {
    always: bool,
    serde: bool,
}

#[derive(Default)]
struct Roots {
    serde_depth: usize,
    b_tree_map: bool,
    convert: Usage,
    fmt: Usage,
    marker: Usage,
    shape: Usage,
    de: Usage,
}

impl Roots {
    fn mark(&mut self, root: &str) {
        let serde = self.serde_depth > 0;
        let usage = match root {
            "convert" => &mut self.convert,
            "fmt" => &mut self.fmt,
            "marker" => &mut self.marker,
            "shape" => &mut self.shape,
            "de" => &mut self.de,
            _ => return,
        };
        if serde {
            usage.serde = true;
        } else {
            usage.always = true;
        }
    }
}

fn is_serde_gate(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("cfg")
        && attr
            .to_token_stream()
            .to_string()
            .contains("feature = \"serde\"")
}

impl VisitMut for Roots {
    fn visit_item_mut(&mut self, item: &mut Item) {
        let attrs = match item {
            Item::Impl(item) => &item.attrs,
            Item::Fn(item) => &item.attrs,
            Item::Struct(item) => &item.attrs,
            Item::Enum(item) => &item.attrs,
            _ => &vec![],
        };
        let gated = attrs.iter().any(is_serde_gate);
        self.serde_depth += usize::from(gated);
        visit_mut::visit_item_mut(self, item);
        self.serde_depth -= usize::from(gated);
    }

    fn visit_attribute_mut(&mut self, attr: &mut syn::Attribute) {
        // Serde helper paths live in string literals of `serde`-gated attributes.
        if attr.path().is_ident("cfg_attr")
            && attr.to_token_stream().to_string().contains("shape::")
        {
            self.shape.serde = true;
        }
        visit_mut::visit_attribute_mut(self, attr);
    }

    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        visit_mut::visit_path_mut(self, path);
        let first = path.segments[0].ident.to_string();
        if path.segments.len() == 1 {
            self.b_tree_map |= first == "BTreeMap";
        } else if path.leading_colon.is_none() {
            self.mark(&first);
        }
    }
}

fn import(usage: Usage, path: &str) -> Option<syn::Item> {
    let gate = match usage {
        Usage { always: true, .. } => "",
        Usage { serde: true, .. } => "#[cfg(feature = \"serde\")] ",
        _ => return None,
    };
    Some(syn::parse_str(&format!("{gate}use {path};")).expect("import path is valid"))
}

pub(super) fn add(mut file: syn::File) -> syn::File {
    let mut roots = Roots::default();
    roots.visit_file_mut(&mut file);

    let mut imports = vec![];
    if roots.b_tree_map {
        imports.push(syn::parse_quote!(
            use std::collections::BTreeMap;
        ));
    }
    let std = [
        ("convert", roots.convert),
        ("fmt", roots.fmt),
        ("marker", roots.marker),
    ];
    let always = std
        .iter()
        .filter(|(_, usage)| usage.always)
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    match always.as_slice() {
        [] => {}
        [name] => imports.extend(import(
            Usage {
                always: true,
                serde: false,
            },
            &format!("std::{name}"),
        )),
        names => imports.extend(import(
            Usage {
                always: true,
                serde: false,
            },
            &format!("std::{{{}}}", names.join(", ")),
        )),
    }
    for (name, usage) in std {
        if !usage.always {
            imports.extend(import(usage, &format!("std::{name}")));
        }
    }
    imports.extend(import(roots.shape, "satay_runtime::shape"));
    imports.extend(import(roots.de, "serde::de"));

    file.items.splice(0..0, imports);
    file
}
