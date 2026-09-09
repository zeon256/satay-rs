//! Applies storage parameters to schema-derived types in the rendered AST.
//!
//! Eligibility comes from the schema graph, not Rust type spellings: constrained
//! strings, API configuration, JSON values, and encoding buffers remain concrete.
use crate::ident::type_ident;
use proc_macro2::TokenTree;
use quote::ToTokens;
use std::collections::BTreeSet;

use syn::{
    Expr, Fields, ImplItem, Item, Meta, PathArguments, Type, parse_quote,
    visit_mut::{self, VisitMut},
};

use crate::model::{Api, ComponentKind, EnumFallback, TypeRef};

pub(super) struct StorageGenerics {
    parameter: syn::Ident,
    models: BTreeSet<String>,
    owned: BTreeSet<String>,
    names: BTreeSet<String>,
    actions: BTreeSet<String>,
}

impl StorageGenerics {
    pub fn new(api: &Api) -> Self {
        let mut models = BTreeSet::new();
        loop {
            let previous = models.len();
            for component in &api.components {
                let generic = match &component.kind {
                    ComponentKind::Struct(fields) => {
                        fields.iter().any(|f| uses_storage(&f.ty, &models))
                    }
                    ComponentKind::Alias(ty) => uses_storage(ty, &models),
                    ComponentKind::Union(union) => {
                        union.variants.iter().any(|v| uses_storage(&v.ty, &models))
                    }
                    ComponentKind::Enum(value) => value.fallback == EnumFallback::OtherString,
                    ComponentKind::Nutype(_) | ComponentKind::Range(_) => false,
                };
                if generic {
                    models.insert(component.rust_name.clone());
                }
            }
            if previous == models.len() {
                break;
            }
        }
        let mut names = models.clone();
        let mut actions = BTreeSet::new();
        for operation in &api.operations {
            if super::input_fields(operation)
                .iter()
                .any(|f| uses_storage(&f.ty, &models))
            {
                names.insert(operation.input_name.clone());
            }
            if operation
                .responses
                .iter()
                .any(|r| r.body.as_ref().is_some_and(|t| uses_storage(t, &models)))
            {
                names.insert(operation.response_name.clone());
            }
            let action = format!("{}Action", type_ident(&operation.fn_name));
            names.insert(action.clone());
            actions.insert(action);
        }
        names.extend(["Api".into(), "RootApi".into()]);
        let reserved = api
            .components
            .iter()
            .map(|c| c.rust_name.as_str())
            .chain(api.constrained_types.iter().map(|c| c.rust_name.as_str()))
            .collect::<BTreeSet<_>>();
        let mut parameter = "S".to_owned();
        let mut suffix = 2;
        while reserved.contains(parameter.as_str()) {
            parameter = format!("S{suffix}");
            suffix += 1;
        }
        Self {
            parameter: super::ident(&parameter),
            owned: owned_models(api, &models),
            models,
            names,
            actions,
        }
    }

    pub fn apply(&self, mut file: syn::File) -> syn::File {
        let storage = &self.parameter;
        let mut extra = vec![];
        let root_fields = file.items.iter().find_map(|item| match item {
            Item::Struct(item)
                if item.ident == "Api"
                    && item
                        .fields
                        .iter()
                        .any(|f| f.ident.as_ref().is_some_and(|i| i == "base_url")) =>
            {
                Some(
                    item.fields
                        .iter()
                        .filter_map(|f| f.ident.clone())
                        .collect::<Vec<_>>(),
                )
            }
            _ => None,
        });
        for item in &mut file.items {
            match item {
                Item::Struct(item) if self.names.contains(&item.ident.to_string()) => {
                    let wire =
                        item.ident != "Api" && !self.actions.contains(&item.ident.to_string());
                    let mut rewrite = Rewrite::new(&self.names, wire, storage);
                    for field in &mut item.fields {
                        rewrite.visit_type_mut(&mut field.ty);
                        rewrite_helper_paths(&mut field.attrs, &item.ident.to_string(), storage);
                    }
                    item.generics
                        .params
                        .push(parse_quote!(#storage: satay_runtime::StringStorage = String));
                    self.add_serde_bounds(&mut item.attrs, &item.ident);
                    if item.ident == "Api" && root_fields.is_some() {
                        let Fields::Named(fields) = &mut item.fields else {
                            unreachable!()
                        };
                        fields.named.push(parse_quote!(__satay_storage: std::marker::PhantomData<fn() -> #storage>));
                    }
                }
                Item::Enum(item) if self.names.contains(&item.ident.to_string()) => {
                    let mut rewrite = Rewrite::new(&self.names, true, storage);
                    for variant in &mut item.variants {
                        for field in &mut variant.fields {
                            rewrite.visit_type_mut(&mut field.ty);
                        }
                    }
                    item.generics
                        .params
                        .push(parse_quote!(#storage: satay_runtime::StringStorage = String));
                    self.add_serde_bounds(&mut item.attrs, &item.ident);
                }
                Item::Type(item) if self.names.contains(&item.ident.to_string()) => {
                    Rewrite::new(&self.names, true, storage).visit_type_mut(&mut item.ty);
                    item.generics.params.push(parse_quote!(#storage = String));
                }
                Item::Impl(item) => self.apply_impl(item, root_fields.as_deref(), &mut extra),
                Item::Fn(item) => {
                    let mut rewrite = Rewrite::new(&self.names, true, storage);
                    rewrite.visit_signature_mut(&mut item.sig);
                    if !rewrite.changed {
                        continue;
                    }
                    rewrite.visit_block_mut(&mut item.block);
                    item.sig
                        .generics
                        .params
                        .push(parse_quote!(#storage: satay_runtime::StringStorage));
                    let name = item.sig.ident.to_string();
                    if name.starts_with("encode_") || name.starts_with("decode_") {
                        codec_bounds(&mut item.sig.generics, storage);
                    }
                }
                _ => {}
            }
        }
        file.items.extend(extra);
        let mut imports = StorageImports::default();
        imports.visit_file_mut(&mut file);
        if imports.marker {
            file.items.insert(
                0,
                parse_quote!(
                    use std::marker;
                ),
            );
        }
        if imports.de {
            file.items.insert(
                0,
                parse_quote!(
                    use serde::de;
                ),
            );
        }
        file
    }

    fn add_serde_bounds(&self, attrs: &mut Vec<syn::Attribute>, name: &syn::Ident) {
        add_serde_bounds(
            attrs,
            &self.parameter,
            self.owned.contains(&name.to_string()),
        );
    }

    fn apply_impl(
        &self,
        item: &mut syn::ItemImpl,
        root_fields: Option<&[syn::Ident]>,
        extra: &mut Vec<Item>,
    ) {
        let storage = &self.parameter;
        let Some(name) = type_name(&item.self_ty) else {
            return;
        };
        if !self.names.contains(&name) {
            return;
        }
        let root = name == "Api" && root_fields.is_some();
        if root && item.trait_.is_none() {
            let position = item
                .items
                .iter()
                .position(|i| matches!(i, ImplItem::Fn(f) if f.sig.ident == "new"));
            if let Some(position) = position {
                let mut new = item.items.remove(position);
                AddMarker.visit_impl_item_mut(&mut new);
                extra.push(parse_quote!(impl Api<String> { #new }));
            }
        }
        let wire = !root;
        let mut rewrite = Rewrite::new(&self.names, wire, storage);
        rewrite.expressions = if self.models.contains(&name) {
            Expressions::ModelStrings
        } else if !self.actions.contains(&name) && name != "Api" {
            Expressions::InputDefaults
        } else {
            Expressions::Unchanged
        };
        rewrite.visit_item_impl_mut(item);
        item.generics
            .params
            .push(parse_quote!(#storage: satay_runtime::StringStorage));
        if let Some((_, path, _)) = &item.trait_ {
            let last = &path.segments.last().unwrap().ident;
            if last == "Serialize" {
                add_bound(&mut item.generics, storage, parse_quote!(serde::Serialize));
            } else if last == "Deserialize" {
                add_bound(
                    &mut item.generics,
                    storage,
                    deserialize_bound(self.owned.contains(&name)),
                );
            }
        }
        if self.actions.contains(&name) || (name == "Api" && !root) {
            codec_bounds(&mut item.generics, storage);
        }
        if root {
            if item
                .trait_
                .as_ref()
                .is_some_and(|(_, p, _)| p.is_ident("Default"))
            {
                item.items = vec![parse_quote!(
                    fn default() -> Self {
                        Api::new().string_storage()
                    }
                )];
            } else {
                let fields = root_fields.as_ref().unwrap();
                item.items.push(parse_quote!(
                    /// Selects dynamic string storage for generated models and actions.
                    pub fn string_storage<T: satay_runtime::StringStorage>(self) -> Api<T> {
                        Api { #(#fields: self.#fields,)* __satay_storage: std::marker::PhantomData }
                    }
                ));
            }
        }
    }
}

fn uses_storage(ty: &TypeRef, names: &BTreeSet<String>) -> bool {
    match ty {
        TypeRef::String | TypeRef::Map(_) => true,
        TypeRef::Array(inner) | TypeRef::Option(inner) => uses_storage(inner, names),
        TypeRef::Named(name) => names.contains(name),
        _ => false,
    }
}

// Lossy decoding materializes a JSON value before decoding the field, so its
// storage must deserialize independently of that temporary value's lifetime.
// Propagate this requirement outward, including through aliases and containers.
fn owned_models(api: &Api, models: &BTreeSet<String>) -> BTreeSet<String> {
    let mut owned = BTreeSet::new();
    loop {
        let previous = owned.len();
        for component in &api.components {
            let needs_owned = match &component.kind {
                ComponentKind::Struct(fields) => fields.iter().any(|field| {
                    (field.treat_error_as_none && uses_storage(&field.ty, models))
                        || contains_owned(&field.ty, &owned)
                }),
                ComponentKind::Alias(ty) => contains_owned(ty, &owned),
                ComponentKind::Union(union) => union
                    .variants
                    .iter()
                    .any(|variant| contains_owned(&variant.ty, &owned)),
                _ => false,
            };
            if needs_owned {
                owned.insert(component.rust_name.clone());
            }
        }
        if previous == owned.len() {
            break;
        }
    }
    for operation in &api.operations {
        if super::input_fields(operation)
            .iter()
            .any(|field| contains_owned(&field.ty, &owned))
        {
            owned.insert(operation.input_name.clone());
        }
        if operation.responses.iter().any(|response| {
            response
                .body
                .as_ref()
                .is_some_and(|ty| contains_owned(ty, &owned))
        }) {
            owned.insert(operation.response_name.clone());
        }
    }
    owned
}

fn contains_owned(ty: &TypeRef, owned: &BTreeSet<String>) -> bool {
    match ty {
        TypeRef::Named(name) => owned.contains(name),
        TypeRef::Array(inner) | TypeRef::Option(inner) | TypeRef::Map(inner) => {
            contains_owned(inner, owned)
        }
        _ => false,
    }
}

fn type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

fn codec_bounds(generics: &mut syn::Generics, storage: &syn::Ident) {
    add_bound(generics, storage, parse_quote!(serde::Serialize));
    add_bound(generics, storage, parse_quote!(serde::de::DeserializeOwned));
}

fn add_bound(generics: &mut syn::Generics, storage: &syn::Ident, bound: syn::TypeParamBound) {
    let parameter = generics
        .type_params_mut()
        .find(|p| p.ident == *storage)
        .unwrap();
    parameter.bounds.push(bound);
}

fn deserialize_bound(owned: bool) -> syn::TypeParamBound {
    if owned {
        parse_quote!(serde::de::DeserializeOwned)
    } else {
        parse_quote!(serde::Deserialize<'de>)
    }
}

fn add_serde_bounds(attrs: &mut Vec<syn::Attribute>, storage: &syn::Ident, owned: bool) {
    if attrs.iter().any(|a| {
        a.to_token_stream()
            .to_string()
            .contains("serde :: Deserialize")
    }) {
        let serialize = format!("{storage}: serde::Serialize");
        let deserialize = if owned {
            format!("{storage}: serde::de::DeserializeOwned")
        } else {
            format!("{storage}: serde::Deserialize<'de>")
        };
        attrs.push(parse_quote!(#[cfg_attr(feature = "serde", serde(bound(
            serialize = #serialize,
            deserialize = #deserialize
        )))]));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Expressions {
    Unchanged,
    InputDefaults,
    ModelStrings,
}

struct Rewrite<'a> {
    names: &'a BTreeSet<String>,
    storage: &'a syn::Ident,
    wire: bool,
    changed: bool,
    expressions: Expressions,
}

impl<'a> Rewrite<'a> {
    fn new(names: &'a BTreeSet<String>, wire: bool, storage: &'a syn::Ident) -> Self {
        Self {
            names,
            storage,
            wire,
            changed: false,
            expressions: Expressions::Unchanged,
        }
    }
}

impl VisitMut for Rewrite<'_> {
    fn visit_expr_mut(&mut self, expression: &mut syn::Expr) {
        // Constrained defaults retain their concrete inner representation.
        if self.expressions == Expressions::InputDefaults
            && let Expr::Call(call) = expression
            && matches!(&*call.func, Expr::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "try_new"))
        {
            return;
        }
        if self.expressions == Expressions::InputDefaults
            && let Expr::Call(call) = expression
            && matches!(&*call.func, Expr::Path(p) if p.path.segments.len() == 2 && p.path.segments[0].ident == "String" && p.path.segments[1].ident == "from")
        {
            let original = call.clone();
            *expression = parse_quote!(#original.into());
            return;
        }
        visit_mut::visit_expr_mut(self, expression);
    }

    fn visit_type_path_mut(&mut self, path: &mut syn::TypePath) {
        let storage = self.storage;
        visit_mut::visit_type_path_mut(self, path);
        if self.wire && path.path.is_ident("String") {
            *path = parse_quote!(#storage);
            self.changed = true;
        } else if let Some(segment) = path.path.segments.last_mut()
            && self.names.contains(&segment.ident.to_string())
        {
            match &mut segment.arguments {
                PathArguments::None => {
                    segment.arguments = PathArguments::AngleBracketed(parse_quote!(<#storage>));
                }
                PathArguments::AngleBracketed(args) => {
                    args.args.push(parse_quote!(#storage));
                }
                PathArguments::Parenthesized(_) => unreachable!(),
            }
            self.changed = true;
        }
    }

    fn visit_expr_path_mut(&mut self, path: &mut syn::ExprPath) {
        let storage = self.storage;
        visit_mut::visit_expr_path_mut(self, path);
        if path.path.segments.len() < 2 {
            return;
        }
        let first = &mut path.path.segments[0];
        if first.ident == "String" && self.expressions == Expressions::ModelStrings {
            // Only model string deserialization and schema parameter defaults.
            first.ident = parse_quote!(#storage);
        } else if self.names.contains(&first.ident.to_string())
            && first.ident != "Api"
            && first.arguments.is_empty()
        {
            first.arguments = PathArguments::AngleBracketed(parse_quote!(::<#storage>));
        }
    }

    fn visit_expr_method_call_mut(&mut self, call: &mut syn::ExprMethodCall) {
        visit_mut::visit_expr_method_call_mut(self, call);
        if self.expressions == Expressions::ModelStrings
            && call.method == "as_str"
            && matches!(&*call.receiver, Expr::Path(p) if p.path.is_ident("value"))
        {
            call.method = parse_quote!(as_ref);
        }
    }
}

struct AddMarker;
impl VisitMut for AddMarker {
    fn visit_expr_struct_mut(&mut self, item: &mut syn::ExprStruct) {
        visit_mut::visit_expr_struct_mut(self, item);
        if item.path.is_ident("Self") {
            item.fields
                .push(parse_quote!(__satay_storage: std::marker::PhantomData));
        }
    }
}

// Serde helper paths live in string attributes and need explicit storage arguments;
// `Self` would refer to Serde's generated wrapper, rather than the model.
fn rewrite_helper_paths(attrs: &mut [syn::Attribute], name: &str, storage: &syn::Ident) {
    fn rewrite(
        tokens: proc_macro2::TokenStream,
        name: &str,
        storage: &syn::Ident,
    ) -> proc_macro2::TokenStream {
        tokens
            .into_iter()
            .map(|token| match token {
                TokenTree::Group(group) => {
                    let mut result = proc_macro2::Group::new(
                        group.delimiter(),
                        rewrite(group.stream(), name, storage),
                    );
                    result.set_span(group.span());
                    TokenTree::Group(result)
                }
                TokenTree::Literal(literal) => {
                    if let Ok(value) = syn::parse_str::<syn::LitStr>(&literal.to_string())
                        && let Some(method) =
                            value.value().strip_prefix(&format!("{name}::__satay_"))
                    {
                        let path = syn::LitStr::new(
                            &format!("{name}::<{storage}>::__satay_{method}"),
                            value.span(),
                        );
                        path.into_token_stream().into_iter().next().unwrap()
                    } else {
                        TokenTree::Literal(literal)
                    }
                }
                other => other,
            })
            .collect()
    }
    for attr in attrs {
        if attr.path().is_ident("cfg_attr")
            && let Meta::List(list) = &mut attr.meta
        {
            list.tokens = rewrite(list.tokens.clone(), name, storage);
        }
    }
}

#[derive(Default)]
struct StorageImports {
    marker: bool,
    de: bool,
}

impl VisitMut for StorageImports {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        visit_mut::visit_path_mut(self, path);
        if path.segments.len() != 3 {
            return;
        }
        if path.segments[0].ident == "std" && path.segments[1].ident == "marker" {
            self.marker = true;
        } else if path.segments[0].ident == "serde" && path.segments[1].ident == "de" {
            self.de = true;
        } else {
            return;
        }
        path.segments = path.segments.iter().skip(1).cloned().collect();
    }
}
