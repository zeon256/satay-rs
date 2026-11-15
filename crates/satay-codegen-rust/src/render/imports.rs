//! Adds the `use` items that a rendered file's short paths rely on.
//!
//! Renderers write `fmt::Debug`, `shape::Text`, or `de::Visitor` so generated
//! paths stay at two segments. This pass finds which roots a file mentions and
//! imports exactly those, under the weakest feature gate any use has.

use quote::ToTokens;
use syn::Item;
use syn::visit_mut::{self, VisitMut};

/// A feature gate, ordered from the narrowest to none: `json` implies `serde`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Gate {
    Json,
    Serde,
    Always,
}

/// The weakest gate a root is used under, if it is used at all.
type Usage = Option<Gate>;

#[derive(Default)]
struct Roots {
    /// Gates of the items enclosing the current position.
    enclosing: Vec<Gate>,
    b_tree_map: bool,
    convert: Usage,
    fmt: Usage,
    marker: Usage,
    shape: Usage,
    de: Usage,
}

impl Roots {
    fn mark(&mut self, root: &str, gate: Gate) {
        let usage = match root {
            "convert" => &mut self.convert,
            "fmt" => &mut self.fmt,
            "marker" => &mut self.marker,
            "shape" => &mut self.shape,
            "de" => &mut self.de,
            _ => return,
        };
        *usage = (*usage).max(Some(gate));
    }
}

fn gate(attr: &syn::Attribute) -> Option<Gate> {
    if !attr.path().is_ident("cfg") {
        return None;
    }
    let cfg = attr.to_token_stream().to_string();
    if cfg.contains("feature = \"json\"") {
        Some(Gate::Json)
    } else if cfg.contains("feature = \"serde\"") {
        Some(Gate::Serde)
    } else {
        None
    }
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
        let gate = attrs.iter().find_map(gate);
        self.enclosing.extend(gate);
        visit_mut::visit_item_mut(self, item);
        if gate.is_some() {
            self.enclosing.pop();
        }
    }

    fn visit_attribute_mut(&mut self, attr: &mut syn::Attribute) {
        // Serde helper paths and bounds live in string literals of
        // `serde`-gated attributes.
        if attr.path().is_ident("cfg_attr") {
            let tokens = attr.to_token_stream().to_string();
            for root in ["shape", "fmt"] {
                if tokens.contains(&format!("{root}::")) {
                    self.mark(root, Gate::Serde);
                }
            }
        }
        visit_mut::visit_attribute_mut(self, attr);
    }

    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        visit_mut::visit_path_mut(self, path);
        let first = path.segments[0].ident.to_string();
        if path.segments.len() == 1 {
            self.b_tree_map |= first == "BTreeMap";
        } else if path.leading_colon.is_none() {
            let gate = self.enclosing.iter().min().copied().unwrap_or(Gate::Always);
            self.mark(&first, gate);
        }
    }
}

fn import(gate: Gate, path: &str) -> syn::Item {
    let gate = match gate {
        Gate::Always => "",
        Gate::Serde => "#[cfg(feature = \"serde\")] ",
        Gate::Json => "#[cfg(feature = \"json\")] ",
    };
    syn::parse_str(&format!("{gate}use {path};")).expect("import path is valid")
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
        .filter(|(_, usage)| *usage == Some(Gate::Always))
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    match always.as_slice() {
        [] => {}
        [name] => imports.push(import(Gate::Always, &format!("std::{name}"))),
        names => imports.push(import(
            Gate::Always,
            &format!("std::{{{}}}", names.join(", ")),
        )),
    }
    for (name, usage) in std {
        if let Some(gate) = usage.filter(|gate| *gate != Gate::Always) {
            imports.push(import(gate, &format!("std::{name}")));
        }
    }
    imports.extend(roots.shape.map(|gate| import(gate, "satay_runtime::shape")));
    imports.extend(roots.de.map(|gate| import(gate, "serde::de")));

    file.items.splice(0..0, imports);
    file
}
