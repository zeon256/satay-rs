//! Applies storage parameters to schema-derived types in the rendered AST.
//!
//! Eligibility comes from the schema graph, not Rust type spellings: constrained
//! strings, API configuration, JSON values, and encoding buffers remain concrete.
use super::storage_traits;
use crate::ident::type_ident;
use crate::model::Operation;
use proc_macro2::TokenTree;
use quote::ToTokens;
use std::collections::BTreeSet;
use syn::{Member, Stmt};

use syn::{
    Expr, Fields, ImplItem, Item, Meta, PathArguments, Type, parse_quote,
    visit_mut::{self, VisitMut},
};

use super::storage_requirements::StorageRequirements;
use crate::model::{Api, ComponentKind, TypeRef};

pub(super) struct StorageGenerics<'api> {
    pub(super) parameter: syn::Ident,
    pub(super) models: BTreeSet<String>,
    owned: BTreeSet<String>,
    pub(super) names: BTreeSet<String>,
    actions: BTreeSet<String>,
    pub(super) api: &'api Api,
}

impl<'api> StorageGenerics<'api> {
    pub fn new(api: &'api Api) -> Self {
        let requirements = StorageRequirements::new(api);
        let models = requirements
            .models
            .iter()
            .filter(|(_, usage)| usage.text || usage.contiguous)
            .map(|(name, _)| name.clone())
            .collect::<BTreeSet<_>>();
        let mut names = models.clone();
        names.extend(
            requirements
                .inputs
                .iter()
                .chain(&requirements.responses)
                .filter(|(_, usage)| usage.text || usage.contiguous)
                .map(|(name, _)| name.clone()),
        );
        let mut actions = BTreeSet::new();
        for operation in &api.operations {
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
            api,
            parameter: super::ident(&parameter),
            owned: owned_models(api, &models),
            models,
            names,
            actions,
        }
    }

    pub(super) fn owned_aliases(&self) -> Item {
        let names = self
            .api
            .components
            .iter()
            .map(|component| component.rust_name.as_str())
            .chain(self.api.operations.iter().flat_map(|operation| {
                [
                    operation.input_name.as_str(),
                    operation.response_name.as_str(),
                ]
            }));
        let aliases = names.map(|name| {
            let ident = super::ident(name);
            if self.names.contains(name) { quote::quote!(pub type #ident = super::#ident<'static, satay_runtime::storage::AllocStorage>;) }
            else { quote::quote!(pub type #ident = super::#ident;) }
        });
        parse_quote!(/// Owned model and operation aliases using the default allocation policy.
            pub mod owned { #(#aliases)* })
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
                    let mut rewrite = Rewrite::new(&self.names, storage);
                    for field in &mut item.fields {
                        rewrite.visit_type_mut(&mut field.ty);
                        rewrite_helper_paths(&mut field.attrs, &item.ident.to_string(), storage);
                    }
                    item.generics
                        .params
                        .push(parse_quote!(#storage: satay_runtime::storage::Storage + 'storage = satay_runtime::storage::AllocStorage));
                    item.generics.params.insert(0, parse_quote!('storage));
                    let fields = item.fields.iter().map(|f| f.ty.clone()).collect::<Vec<_>>();
                    self.add_serde_bounds(&mut item.attrs, &item.ident, &fields);
                    if item.ident == "Api" && root_fields.is_some() {
                        let Fields::Named(fields) = &mut item.fields else {
                            unreachable!()
                        };
                        fields
                            .named
                            .push(parse_quote!(__satay_storage: &'storage #storage));
                    }
                }
                Item::Enum(item) if self.names.contains(&item.ident.to_string()) => {
                    let mut rewrite = Rewrite::new(&self.names, storage);
                    for variant in &mut item.variants {
                        for field in &mut variant.fields {
                            rewrite.visit_type_mut(&mut field.ty);
                        }
                    }
                    item.generics
                        .params
                        .push(parse_quote!(#storage: satay_runtime::storage::Storage + 'storage = satay_runtime::storage::AllocStorage));
                    item.generics.params.insert(0, parse_quote!('storage));
                    let fields = item
                        .variants
                        .iter()
                        .flat_map(|v| &v.fields)
                        .map(|f| f.ty.clone())
                        .collect::<Vec<_>>();
                    self.add_serde_bounds(&mut item.attrs, &item.ident, &fields);
                }
                Item::Type(item) if self.names.contains(&item.ident.to_string()) => {
                    Rewrite::new(&self.names, storage).visit_type_mut(&mut item.ty);
                    item.generics.params.insert(0, parse_quote!('storage));
                    item.generics
                        .params
                        .push(parse_quote!(#storage = satay_runtime::storage::AllocStorage));
                }
                Item::Impl(item) => self.apply_impl(item, root_fields.as_deref(), &mut extra),
                Item::Fn(item) => {
                    let mut rewrite = Rewrite::new(&self.names, storage);
                    rewrite.visit_signature_mut(&mut item.sig);
                    if !rewrite.changed {
                        continue;
                    }
                    rewrite.visit_block_mut(&mut item.block);
                    item.sig
                        .generics
                        .params
                        .push(parse_quote!(#storage: satay_runtime::storage::Storage + 'storage));
                    item.sig.generics.params.insert(0, parse_quote!('storage));
                    let name = item.sig.ident.to_string();
                    if name.starts_with("encode_") || name.starts_with("decode_") {
                        self.function_bounds(item);
                    }
                }
                _ => {}
            }
        }
        self.finish_file(file, extra)
    }

    fn finish_file(&self, mut file: syn::File, mut extra: Vec<Item>) -> syn::File {
        for item in &mut file.items {
            match item {
                Item::Struct(value) if self.names.contains(&value.ident.to_string()) => {
                    if let Some(implementations) = self.model_value_traits(&value.ident) {
                        value.attrs.retain(|attr| !attr.path().is_ident("derive"));
                        extra.extend(implementations);
                    } else {
                        extra.extend(storage_traits::struct_impls(value));
                    }
                }
                Item::Enum(value) if self.names.contains(&value.ident.to_string()) => {
                    if let Some(implementations) = self.model_value_traits(&value.ident) {
                        value.attrs.retain(|attr| !attr.path().is_ident("derive"));
                        extra.extend(implementations);
                    } else {
                        extra.extend(storage_traits::enum_impls(value));
                    }
                }
                _ => {}
            }
        }
        self.owned_deserializers(&mut file, &mut extra);
        extra.extend(self.field_serializers(&mut file));
        extra.extend(self.model_seeds(&file));
        extra.extend(self.context_decoders(&file));
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
                    #[cfg(feature = "serde")]
                    use serde::de;
                ),
            );
        }
        file
    }

    fn add_serde_bounds(
        &self,
        attrs: &mut Vec<syn::Attribute>,
        name: &syn::Ident,
        fields: &[Type],
    ) {
        if attrs.iter().any(|a| {
            a.to_token_stream()
                .to_string()
                .contains("serde :: Deserialize")
        }) {
            let serialize = String::new();
            let deserialize = fields
                .iter()
                .map(|ty| {
                    {
                        if self.owned.contains(&name.to_string()) {
                            quote::quote!(#ty: serde::de::DeserializeOwned)
                        } else {
                            quote::quote!(#ty: serde::Deserialize<'de>)
                        }
                    }
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join(", ");
            attrs.push(parse_quote!(#[cfg_attr(feature = "serde", serde(bound(serialize = #serialize, deserialize = #deserialize)))]));
        }
    }

    fn function_bounds(&self, item: &mut syn::ItemFn) {
        let name = item.sig.ident.to_string();
        for operation in &self.api.operations {
            if name == format!("encode_{}", operation.fn_name) {
                let expression = self.request_expression(operation);
                if let Some(Stmt::Expr(tail, None)) = item.block.stmts.last_mut() {
                    *tail = expression;
                }
            } else if name == format!("decode_{}_response", operation.fn_name) {
                self.response_bounds(&mut item.sig.generics, operation);
            }
        }
    }

    pub(super) fn ty(&self, ty: &TypeRef) -> Type {
        struct Qualify<'a>(&'a Api);
        impl VisitMut for Qualify<'_> {
            fn visit_type_path_mut(&mut self, path: &mut syn::TypePath) {
                visit_mut::visit_type_path_mut(self, path);
                if path.path.segments.len() == 1
                    && self
                        .0
                        .components
                        .iter()
                        .any(|component| path.path.is_ident(&component.rust_name))
                {
                    path.path.segments.insert(0, parse_quote!(self));
                }
            }
        }
        let mut ty = super::rust_type(ty);
        Qualify(self.api).visit_type_mut(&mut ty);
        Rewrite::new(&self.names, &self.parameter).visit_type_mut(&mut ty);
        ty
    }

    fn response_bounds(&self, generics: &mut syn::Generics, operation: &Operation) {
        for response in &operation.responses {
            if let Some(body) = &response.body {
                let ty = self.ty(body);
                generics
                    .make_where_clause()
                    .predicates
                    .push(parse_quote!(#ty: serde::de::DeserializeOwned));
            }
        }
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
                extra.push(
                    parse_quote!(impl Api<'static, satay_runtime::storage::AllocStorage> { #new }),
                );
            }
        }
        let mut rewrite = Rewrite::new(&self.names, storage);
        rewrite.expressions =
            if !self.models.contains(&name) && !self.actions.contains(&name) && name != "Api" {
                Expressions::InputDefaults
            } else {
                Expressions::Unchanged
            };
        rewrite.visit_item_impl_mut(item);
        item.generics
            .params
            .push(parse_quote!(#storage: satay_runtime::storage::Storage + 'storage));
        item.generics.params.insert(0, parse_quote!('storage));
        if let Some((_, path, _)) = &item.trait_ {
            let last = &path.segments.last().unwrap().ident;
            if last == "Deserialize" {
                let text: Type =
                    parse_quote!(<#storage as satay_runtime::storage::Storage>::Text<'storage>);
                item.generics
                    .make_where_clause()
                    .predicates
                    .push(parse_quote!(#text: serde::Deserialize<'de>));
            }
        }
        if self.actions.contains(&name) {
            let operation = self
                .api
                .operations
                .iter()
                .find(|operation| format!("{}Action", type_ident(&operation.fn_name)) == name)
                .unwrap();
            if item.trait_.is_some() {
                self.response_bounds(&mut item.generics, operation);
            } else {
                for member in &mut item.items {
                    if let ImplItem::Fn(method) = member {
                        if method.sig.ident == "request" {
                            let expression = self.request_expression(operation);
                            if let Some(Stmt::Expr(tail, None)) = method.block.stmts.last_mut() {
                                *tail = expression;
                            }
                        }
                        if method.sig.ident == "decode" {
                            self.response_bounds(&mut method.sig.generics, operation);
                        }
                    }
                }
            }
        }
        if self.actions.contains(&name) && item.trait_.is_none() {
            let operation = self
                .api
                .operations
                .iter()
                .find(|operation| format!("{}Action", type_ident(&operation.fn_name)) == name)
                .unwrap();
            self.action_constructor(item, operation);
        }
        if name == "Api" && !root && item.trait_.is_none() {
            self.group_constructors(item);
        }
        if let Some(operation) = self
            .api
            .operations
            .iter()
            .find(|operation| operation.input_name == name)
        {
            self.input_constructors(item, operation, extra);
        }
        if root {
            self.root_impl(item, root_fields.unwrap());
        }
    }

    fn root_impl(&self, item: &mut syn::ItemImpl, fields: &[syn::Ident]) {
        let storage = &self.parameter;
        if item
            .trait_
            .as_ref()
            .is_some_and(|(_, p, _)| p.is_ident("Default"))
        {
            item.generics
                .make_where_clause()
                .predicates
                .push(parse_quote!(#storage: satay_runtime::StaticStorage));
            item.items = vec![parse_quote!(
                fn default() -> Self {
                    Api::new().storage()
                }
            )];
        } else {
            item.items.push(parse_quote!(
                /// Selects a static storage family for generated models and actions.
                pub fn storage<T: satay_runtime::StaticStorage>(self) -> Api<'static, T> {
                    self.storage_in(T::context())
                }
            ));
            item.items.push(parse_quote!(
                    /// Selects a borrowed context for generated inputs and responses.
                    pub fn storage_in<T: satay_runtime::storage::Storage>(self, storage: &T) -> Api<'_, T> {
                        Api { #(#fields: self.#fields,)* __satay_storage: storage }
                    }
                ));
        }
    }
}

fn uses_storage(ty: &TypeRef, names: &BTreeSet<String>) -> bool {
    match ty {
        TypeRef::String | TypeRef::Array(_) => true,
        TypeRef::Map(inner) | TypeRef::Option(inner) => uses_storage(inner, names),
        TypeRef::Named(name) => names.contains(name),
        TypeRef::Coordinates(codec) => names.contains(codec.target()),
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
        TypeRef::Coordinates(codec) => owned.contains(codec.target()),
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Expressions {
    Unchanged,
    InputDefaults,
}

struct Rewrite<'a> {
    names: &'a BTreeSet<String>,
    storage: &'a syn::Ident,
    changed: bool,
    expressions: Expressions,
}

impl<'a> Rewrite<'a> {
    fn new(names: &'a BTreeSet<String>, storage: &'a syn::Ident) -> Self {
        Self {
            names,
            storage,
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
        if path.path.is_ident("__SatayText") {
            *path = parse_quote!(<#storage as satay_runtime::storage::Storage>::Text<'storage>);
            self.changed = true;
        } else if path
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "__SatayContiguous")
        {
            let PathArguments::AngleBracketed(args) = &path.path.segments.last().unwrap().arguments
            else {
                unreachable!()
            };
            let element = args.args.first().unwrap();
            *path = parse_quote!(<#storage as satay_runtime::storage::Storage>::Contiguous<'storage, #element>);
            self.changed = true;
        } else if let Some(segment) = path.path.segments.last_mut()
            && self.names.contains(&segment.ident.to_string())
        {
            match &mut segment.arguments {
                PathArguments::None => {
                    segment.arguments =
                        PathArguments::AngleBracketed(parse_quote!(<'storage, #storage>));
                }
                PathArguments::AngleBracketed(args) => {
                    args.args.insert(0, parse_quote!('storage));
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
        if self.names.contains(&first.ident.to_string())
            && first.ident != "Api"
            && first.arguments.is_empty()
        {
            first.arguments = PathArguments::AngleBracketed(parse_quote!(::<#storage>));
        }
    }
}

struct AddMarker;
impl VisitMut for AddMarker {
    fn visit_expr_struct_mut(&mut self, item: &mut syn::ExprStruct) {
        visit_mut::visit_expr_struct_mut(self, item);
        if item.path.is_ident("Self") {
            item.fields
                .push(parse_quote!(__satay_storage: <satay_runtime::storage::AllocStorage as satay_runtime::StaticStorage>::context()));
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
    fn visit_field_value_mut(&mut self, field: &mut syn::FieldValue) {
        visit_mut::visit_field_value_mut(self, field);
        if let Member::Named(name) = &field.member
            && matches!(&field.expr, Expr::Path(path) if path.path.is_ident(name))
        {
            field.colon_token = None;
        }
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        visit_mut::visit_expr_mut(self, expr);
        // Schema-recursive callbacks sometimes reduce to a direct call or identity.
        if let Expr::Closure(closure) = expr
            && let Expr::Call(call) = &*closure.body
            && closure.inputs.len() == 1
            && call.args.len() == 1
            && closure.inputs[0].to_token_stream().to_string()
                == call.args[0].to_token_stream().to_string()
        {
            *expr = (*call.func).clone();
        } else if let Expr::MethodCall(call) = expr
            && call.method == "map"
            && call.args.len() == 1
            && let Expr::Closure(closure) = &call.args[0]
            && closure.inputs.len() == 1
            && closure.inputs[0].to_token_stream().to_string()
                == closure.body.to_token_stream().to_string()
        {
            *expr = (*call.receiver).clone();
        }
    }

    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        visit_mut::visit_path_mut(self, path);
        if path.segments.len() < 3 {
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

pub(super) fn concrete_type(ty: &mut Type) {
    struct Concrete;
    impl VisitMut for Concrete {
        fn visit_type_path_mut(&mut self, path: &mut syn::TypePath) {
            visit_mut::visit_type_path_mut(self, path);
            if let Some(segment) = path.path.segments.last_mut() {
                if segment.ident == "__SatayText" {
                    segment.ident = parse_quote!(String);
                }
                if segment.ident == "__SatayContiguous" {
                    segment.ident = parse_quote!(Vec);
                }
            }
        }
    }
    Concrete.visit_type_mut(ty);
}
