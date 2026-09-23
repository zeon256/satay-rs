use super::{ident, storage::StorageGenerics};
use crate::model::{ComponentKind, TypeRef};
use quote::quote;
use syn::{Item, Type, parse_quote};
use syn::{Meta, parse::Parser, punctuated};

impl StorageGenerics<'_> {
    pub(super) fn serialization_codec(&self, ty: &TypeRef) -> Type {
        match ty {
            TypeRef::String => parse_quote!(satay_runtime::storage_serde::serialize::Text),
            TypeRef::Array(inner) => {
                let codec = self.serialization_codec(inner);
                let inner = self.ty(inner);
                parse_quote!(satay_runtime::storage_serde::serialize::Array<#codec, #inner>)
            }
            TypeRef::Option(inner) => {
                let codec = self.serialization_codec(inner);
                parse_quote!(satay_runtime::storage_serde::serialize::Optional<#codec>)
            }
            TypeRef::Map(inner) => {
                let codec = self.serialization_codec(inner);
                parse_quote!(satay_runtime::storage_serde::serialize::Map<#codec>)
            }
            TypeRef::Named(name) => {
                if let Some(component) = self
                    .api
                    .components
                    .iter()
                    .find(|component| &component.rust_name == name)
                    && let ComponentKind::Alias(inner) = &component.kind
                {
                    return self.serialization_codec(inner);
                }
                parse_quote!(satay_runtime::storage_serde::serialize::Native)
            }
            _ => parse_quote!(satay_runtime::storage_serde::serialize::Native),
        }
    }

    pub(super) fn field_serializers(&self, file: &mut syn::File) -> Vec<Item> {
        let mut output = vec![];
        for component in &self.api.components {
            if !self.models.contains(&component.rust_name) {
                continue;
            }
            let name = ident(&component.rust_name);
            let Some(item) = file.items.iter_mut().find(|item| match item {
                Item::Struct(item) => item.ident == name,
                Item::Enum(item) => item.ident == name,
                _ => false,
            }) else {
                continue;
            };
            let mut helpers = vec![];
            match (&component.kind, item) {
                (ComponentKind::Struct(fields), Item::Struct(item)) => {
                    for (field, rendered) in fields.iter().zip(item.fields.iter_mut()) {
                        if !self.uses_family(&field.ty) {
                            continue;
                        }
                        // Special coordinate/scalar codecs keep their existing helpers.
                        if matches!(field.ty.non_option(), TypeRef::Coordinates(_)) {
                            continue;
                        }
                        let schema = if (!field.required || field.treat_error_as_none)
                            && !field.ty.is_option()
                        {
                            TypeRef::Option(Box::new(field.ty.clone()))
                        } else {
                            field.ty.clone()
                        };
                        self.serializer_field(
                            &name,
                            rendered,
                            &schema,
                            helpers.len(),
                            &mut helpers,
                        );
                    }
                }
                (ComponentKind::Union(union), Item::Enum(item)) => {
                    for (variant, rendered) in union.variants.iter().zip(&mut item.variants) {
                        self.serializer_field(
                            &name,
                            rendered.fields.iter_mut().next().unwrap(),
                            &variant.ty,
                            helpers.len(),
                            &mut helpers,
                        );
                    }
                }
                _ => {}
            }
            if !helpers.is_empty() {
                let storage = &self.parameter;
                output.push(parse_quote!(#[cfg(feature = "serde")] impl<'storage, #storage: satay_runtime::storage::Storage + 'storage> #name<'storage, #storage> { #(#helpers)* }));
            }
        }
        output
    }

    pub(super) fn uses_family(&self, ty: &TypeRef) -> bool {
        match ty {
            TypeRef::String | TypeRef::Array(_) => true,
            TypeRef::Map(inner) | TypeRef::Option(inner) => self.uses_family(inner),
            TypeRef::Named(name) => self.models.contains(name),
            _ => false,
        }
    }

    fn serializer_field(
        &self,
        name: &syn::Ident,
        field: &mut syn::Field,
        ty: &TypeRef,
        index: usize,
        helpers: &mut Vec<syn::ImplItemFn>,
    ) {
        // Lossy fields already have a serializer; keep their deserializer but
        // replace the serialization half with the policy-independent adapter.
        for attr in &mut field.attrs {
            if let Meta::List(list) = &mut attr.meta
                && list.path.is_ident("cfg_attr")
            {
                let tokens = list.tokens.to_string();
                if tokens.contains("serialize_with") {
                    let parsed = Parser::parse2(
                        punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                        list.tokens.clone(),
                    )
                    .unwrap();
                    let rebuilt = parsed.into_iter().map(|mut meta| {
                        if let Meta::List(list) = &mut meta && list.path.is_ident("serde") {
                            let entries = Parser::parse2(punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated, list.tokens.clone()).unwrap();
                            let entries = entries.into_iter().filter(|entry| !entry.path().is_ident("serialize_with"));
                            list.tokens = quote!(#(#entries),*);
                        }
                        meta
                    });
                    list.tokens = quote!(#(#rebuilt),*);
                }
            }
        }
        let method = ident(&format!("__satay_serialize_storage_{index}"));
        let storage = &self.parameter;
        let path = format!("{name}::<{storage}>::{method}");
        field
            .attrs
            .push(parse_quote!(#[cfg_attr(feature = "serde", serde(serialize_with = #path))]));
        let codec = self.serialization_codec(ty);
        let ty = &field.ty;
        helpers.push(parse_quote! {
            fn #method<Serializer: serde::Serializer>(value: &#ty, serializer: Serializer) -> Result<Serializer::Ok, Serializer::Error> {
                <#codec as satay_runtime::storage_serde::serialize::Codec<#ty>>::serialize(value, serializer)
            }
        });
    }
}
