//! Constructors bind allocation to an explicit context; owned constructors stay
//! specialized so associated-type projections do not break policy inference.
use super::endpoint::input;
use super::{ident, input_fields, storage::StorageGenerics};
use crate::model::{Operation, ParameterDefault, TypeRef};
use quote::{ToTokens, quote};
use syn::{FnArg, GenericArgument, PathArguments, ReturnType};
use syn::{
    ImplItem, Item, Type, parse_quote,
    visit_mut::{self, VisitMut},
};

impl StorageGenerics<'_> {
    pub(super) fn input_constructors(
        &self,
        item: &mut syn::ItemImpl,
        operation: &Operation,
        extra: &mut Vec<Item>,
    ) {
        let storage = &self.parameter;
        if item.trait_.is_some() {
            specialize(item, storage);
            return;
        }
        let Some(position) = item
            .items
            .iter()
            .position(|item| matches!(item, ImplItem::Fn(method) if method.sig.ident == "new"))
        else {
            return;
        };
        let original = item.items.remove(position);
        let name = ident(&operation.input_name);
        let mut owned: syn::ItemImpl = parse_quote!(impl<'storage, #storage: satay_runtime::storage::Storage + 'storage> #name<'storage, #storage> { #original });
        specialize(&mut owned, storage);
        extra.push(Item::Impl(owned));
        let fields = input_fields(operation);
        let args = fields.iter().filter(|field| field.required).map(|field| {
            let name = ident(&field.rust_name);
            let ty = self.ty(&field.ty);
            if field.ty == TypeRef::String {
                quote!(#name: impl AsRef<str>)
            } else {
                quote!(#name: #ty)
            }
        });
        let initializers = fields.iter().map(|field| {
            let name = ident(&field.rust_name);
            let value = if field.required {
                if field.ty == TypeRef::String {
                    quote!(__satay_context.try_text(#name.as_ref())?)
                } else {
                    quote!(#name)
                }
            } else if let Some(default) = operation
                .parameters
                .iter()
                .find(|p| p.rust_name == field.rust_name)
                .and_then(|p| p.default.as_ref())
            {
                let value = Self::context_default(default, &field.ty);
                quote!(Some(#value))
            } else {
                quote!(None)
            };
            quote!(#name: #value)
        });
        item.items.push(parse_quote! {
            /// Constructs inputs using this context, including allocating schema defaults.
            pub fn try_new_in(__satay_context: &'storage #storage, #(#args),*) -> Result<Self, #storage::Error> {
                Ok(Self { #(#initializers),* })
            }
        });
    }

    fn context_default(default: &ParameterDefault, ty: &TypeRef) -> proc_macro2::TokenStream {
        match (default, ty) {
            (ParameterDefault::String(value), TypeRef::String) => {
                quote!(__satay_context.try_text(#value)?)
            }
            (ParameterDefault::OpenEnum(value), TypeRef::Named(name)) => {
                let name = ident(name);
                quote!(#name::Other(__satay_context.try_text(#value)?))
            }
            _ => input::render_parameter_default(default, ty),
        }
    }

    pub(super) fn action_constructor(&self, item: &mut syn::ItemImpl, operation: &Operation) {
        let storage = &self.parameter;
        let input = ident(&operation.input_name);
        let Some(position) = item
            .items
            .iter()
            .position(|item| matches!(item, ImplItem::Fn(method) if method.sig.ident == "new"))
        else {
            return;
        };
        let ImplItem::Fn(mut original) = item.items.remove(position) else {
            unreachable!()
        };
        text_arguments(&mut original.sig);
        let args = original
            .sig
            .inputs
            .iter()
            .map(|arg| match arg {
                FnArg::Typed(arg) => arg.pat.to_token_stream(),
                FnArg::Receiver(_) => unreachable!(),
            })
            .collect::<Vec<_>>();
        let api = &args[0];
        let fields = &args[1..];
        let input_value = if self.names.contains(&operation.input_name) {
            quote!(#input::try_new_in(#api.__satay_storage, #(#fields),*)?)
        } else {
            quote!(#input::new(#(#fields),*))
        };
        let mut fallible = original.clone();
        // A uniform fallible constructor also covers operations without allocating fields.
        fallible
            .attrs
            .push(parse_quote!(#[allow(clippy::unnecessary_wraps)]));
        fallible.sig.ident = ident("try_new");
        fallible.sig.output = parse_quote!(-> Result<Self, #storage::Error>);
        fallible.block = parse_quote!({ Ok(Self { api: #api, input: #input_value }) });
        original.sig.generics.make_where_clause().predicates.push(parse_quote!(#storage: satay_runtime::storage::Storage<Error = core::convert::Infallible>));
        original.block = parse_quote!({ match Self::try_new(#(#args),*) { Ok(value) => value, Err(never) => match never {} } });
        item.items.insert(position, ImplItem::Fn(original));
        item.items.push(ImplItem::Fn(fallible));
    }

    pub(super) fn group_constructors(&self, item: &mut syn::ItemImpl) {
        struct TryNew;
        impl VisitMut for TryNew {
            fn visit_expr_path_mut(&mut self, path: &mut syn::ExprPath) {
                if let Some(last) = path.path.segments.last_mut()
                    && last.ident == "new"
                {
                    last.ident = ident("try_new");
                }
            }
        }
        let storage = &self.parameter;
        let mut extra = vec![];
        for member in &mut item.items {
            let ImplItem::Fn(method) = member else {
                continue;
            };
            text_arguments(&mut method.sig);
            let mut fallible = method.clone();
            fallible.sig.ident = ident(&format!(
                "try_{}",
                method.sig.ident.to_string().trim_start_matches("r#")
            ));
            let ReturnType::Type(_, result) = &method.sig.output else {
                unreachable!()
            };
            fallible.sig.output = parse_quote!(-> Result<#result, #storage::Error>);
            TryNew.visit_block_mut(&mut fallible.block);
            method.sig.generics.make_where_clause().predicates.push(parse_quote!(#storage: satay_runtime::storage::Storage<Error = core::convert::Infallible>));
            extra.push(ImplItem::Fn(fallible));
        }
        item.items.extend(extra);
    }
}

fn text_arguments(signature: &mut syn::Signature) {
    for arg in &mut signature.inputs {
        if let FnArg::Typed(arg) = arg
            && matches!(&*arg.ty, Type::ImplTrait(_))
        {
            *arg.ty = parse_quote!(impl AsRef<str>);
        }
    }
}

fn specialize(item: &mut syn::ItemImpl, storage: &syn::Ident) {
    struct Owned<'a>(&'a syn::Ident);
    impl VisitMut for Owned<'_> {
        fn visit_type_mut(&mut self, ty: &mut Type) {
            if matches!(ty, Type::Path(path) if path.path.is_ident(self.0)) {
                *ty = parse_quote!(satay_runtime::storage::AllocStorage);
            } else {
                visit_mut::visit_type_mut(self, ty);
                if let Type::Path(path) = ty
                    && path.qself.as_ref().is_some_and(|q| matches!(&*q.ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "AllocStorage"))) {
                    let segment = path.path.segments.last().unwrap();
                    if segment.ident == "Text" { *ty = parse_quote!(String); }
                    else if segment.ident == "Contiguous"
                        && let PathArguments::AngleBracketed(args) = &segment.arguments
                        && let Some(GenericArgument::Type(inner)) = args.args.last() {
                        *ty = parse_quote!(Vec<#inner>);
                    }
                }
            }
        }
        fn visit_lifetime_mut(&mut self, lifetime: &mut syn::Lifetime) {
            if lifetime.ident == "storage" {
                *lifetime = parse_quote!('static);
            }
        }
    }
    item.generics = syn::Generics::default();
    Owned(storage).visit_item_impl_mut(item);
}
