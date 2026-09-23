//! Value operations follow schema shapes to terminate at recursive references.
use super::{ident, storage::StorageGenerics, storage_traits};
use crate::model::{ComponentKind, EnumFallback, TypeRef};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Item, parse_quote};

impl StorageGenerics<'_> {
    pub(super) fn model_value_traits(&self, name: &syn::Ident) -> Option<Vec<Item>> {
        let component = self
            .api
            .components
            .iter()
            .find(|component| *name == component.rust_name)?;
        let storage = &self.parameter;
        let (clone, debug, eq) = match &component.kind {
            ComponentKind::Struct(fields) => {
                let clones = fields.iter().map(|field| {
                    let name = ident(&field.rust_name);
                    let ty = if (!field.required
                        || field.treat_error_as_none
                        || !field.none_if.is_empty())
                        && !field.ty.is_option()
                    {
                        TypeRef::Option(Box::new(field.ty.clone()))
                    } else {
                        field.ty.clone()
                    };
                    let value = self.clone_value(&ty, quote!(&self.#name));
                    quote!(#name: #value)
                });
                let debugs = fields.iter().map(|field| {
                    let name = ident(&field.rust_name);
                    let label = &field.rust_name;
                    let ty = if (!field.required
                        || field.treat_error_as_none
                        || !field.none_if.is_empty())
                        && !field.ty.is_option()
                    {
                        TypeRef::Option(Box::new(field.ty.clone()))
                    } else {
                        field.ty.clone()
                    };
                    let value = self.debug_value(&ty, quote!(&self.#name));
                    quote!(builder.field(#label, &#value);)
                });
                let equals = fields.iter().map(|field| {
                    let name = ident(&field.rust_name);
                    let ty = if (!field.required
                        || field.treat_error_as_none
                        || !field.none_if.is_empty())
                        && !field.ty.is_option()
                    {
                        TypeRef::Option(Box::new(field.ty.clone()))
                    } else {
                        field.ty.clone()
                    };
                    self.eq_value(&ty, quote!(&self.#name), quote!(&other.#name))
                });
                (
                    quote!(Self { #(#clones),* }),
                    quote!({ let mut builder = f.debug_struct(stringify!(#name)); #(#debugs)* builder.finish() }),
                    storage_traits::conjunction(equals),
                )
            }
            ComponentKind::Enum(value) if value.fallback == EnumFallback::OtherString => {
                let names = value
                    .variants
                    .iter()
                    .map(|v| ident(&v.rust_name))
                    .collect::<Vec<_>>();
                (
                    quote!(match self { #(Self::#names => Self::#names,)* Self::Other(value) => Self::Other(#storage::clone_text(value)) }),
                    quote!(match self { #(Self::#names => f.write_str(stringify!(#names)),)* Self::Other(value) => f.debug_tuple("Other").field(&AsRef::<str>::as_ref(value)).finish() }),
                    quote!(match (self, other) { #((Self::#names, Self::#names) => true,)* (Self::Other(left), Self::Other(right)) => AsRef::<str>::as_ref(left) == AsRef::<str>::as_ref(right), _ => false }),
                )
            }
            ComponentKind::Union(union) => {
                let clones = union.variants.iter().map(|variant| {
                    let name = ident(&variant.rust_name);
                    let value = self.clone_value(&variant.ty, quote!(value));
                    quote!(Self::#name(value) => Self::#name(#value))
                });
                let debugs = union.variants.iter().map(|variant| { let name=ident(&variant.rust_name); let value=self.debug_value(&variant.ty,quote!(value)); quote!(Self::#name(value) => f.debug_tuple(stringify!(#name)).field(&#value).finish()) });
                let equals = union.variants.iter().map(|variant| {
                    let name = ident(&variant.rust_name);
                    let eq = self.eq_value(&variant.ty, quote!(left), quote!(right));
                    quote!((Self::#name(left),Self::#name(right)) => #eq)
                });
                (
                    quote!(match self { #(#clones),* }),
                    quote!(match self { #(#debugs),* }),
                    quote!(match (self,other) { #(#equals,)* _ => false }),
                )
            }
            _ => return None,
        };
        let mut result = vec![
            parse_quote!(impl<'storage, #storage: satay_runtime::storage_value::CloneStorage + 'storage> Clone for #name<'storage,#storage> { fn clone(&self) -> Self { #clone } }),
            parse_quote!(impl<'storage, #storage: satay_runtime::storage::Storage + 'storage> core::fmt::Debug for #name<'storage,#storage> { fn fmt(&self,f:&mut core::fmt::Formatter<'_>)->core::fmt::Result { #debug } }),
            parse_quote!(impl<'storage, #storage: satay_runtime::storage::Storage + 'storage> PartialEq for #name<'storage,#storage> { fn eq(&self,other:&Self)->bool { #eq } }),
        ];
        if matches!(component.kind, ComponentKind::Enum(_)) {
            result.push(parse_quote!(impl<'storage,#storage:satay_runtime::storage::Storage+'storage> Eq for #name<'storage,#storage> {}));
        }
        Some(result)
    }

    fn alias(&self, ty: &TypeRef) -> Option<&TypeRef> {
        if let TypeRef::Named(name) = ty
            && let Some(component) = self.api.components.iter().find(|c| &c.rust_name == name)
            && let ComponentKind::Alias(ty) = &component.kind
        {
            Some(ty)
        } else {
            None
        }
    }

    fn clone_value(&self, ty: &TypeRef, value: TokenStream) -> TokenStream {
        if let Some(ty) = self.alias(ty) {
            return self.clone_value(ty, value);
        }
        let storage = &self.parameter;
        match ty {
            TypeRef::String => quote!(#storage::clone_text(#value)),
            TypeRef::Array(inner) => {
                let inner = self.clone_value(inner, quote!(element));
                quote!(#storage::clone_contiguous(#value,|element| #inner))
            }
            TypeRef::Option(inner) => {
                let inner = self.clone_value(inner, quote!(element));
                quote!((#value).as_ref().map(|element| #inner))
            }
            TypeRef::Map(inner) => {
                let inner = self.clone_value(inner, quote!(element));
                quote!((#value).iter().map(|(key,element)|(key.clone(),#inner)).collect())
            }
            _ => quote!(Clone::clone(#value)),
        }
    }
    fn debug_value(&self, ty: &TypeRef, value: TokenStream) -> TokenStream {
        if let Some(ty) = self.alias(ty) {
            return self.debug_value(ty, value);
        }
        match ty {
            TypeRef::String => quote!(AsRef::<str>::as_ref(#value)),
            TypeRef::Array(inner) => {
                let inner_ty = self.ty(inner);
                let inner = self.debug_value(inner, quote!(element));
                quote!(satay_runtime::storage_value::DebugWith::new(#value, |values: &_, f: &mut core::fmt::Formatter<'_>| f.debug_list().entries(AsRef::<[#inner_ty]>::as_ref(*values).iter().map(|element| #inner)).finish()))
            }
            TypeRef::Option(inner) => {
                let inner = self.debug_value(inner, quote!(element));
                quote!((#value).as_ref().map(|element| #inner))
            }
            TypeRef::Map(inner) => {
                let map_ty = self.ty(ty);
                let inner = self.debug_value(inner, quote!(element));
                quote!(satay_runtime::storage_value::DebugWith::new(#value,|values: &&#map_ty,f:&mut core::fmt::Formatter<'_>| f.debug_map().entries(values.iter().map(|(key,element)|(key,#inner))).finish()))
            }
            _ => value,
        }
    }
    fn eq_value(&self, ty: &TypeRef, left: TokenStream, right: TokenStream) -> TokenStream {
        if let Some(ty) = self.alias(ty) {
            return self.eq_value(ty, left, right);
        }
        match ty {
            TypeRef::String => quote!(AsRef::<str>::as_ref(#left)==AsRef::<str>::as_ref(#right)),
            TypeRef::Array(inner) => {
                let inner_ty = self.ty(inner);
                let eq = self.eq_value(inner, quote!(left), quote!(right));
                quote!({let left=AsRef::<[#inner_ty]>::as_ref(#left);let right=AsRef::<[#inner_ty]>::as_ref(#right);left.len()==right.len()&&left.iter().zip(right).all(|(left,right)|#eq)})
            }
            TypeRef::Option(inner) => {
                let eq = self.eq_value(inner, quote!(left), quote!(right));
                quote!(match (#left,#right){(Some(left),Some(right))=>#eq,(None,None)=>true,_=>false})
            }
            TypeRef::Map(inner) => {
                let eq = self.eq_value(inner, quote!(left), quote!(right));
                quote!((#left).len()==(#right).len()&&(#left).iter().all(|(key,left)|(#right).get(key).is_some_and(|right|#eq)))
            }
            _ => quote!(PartialEq::eq(#left, #right)),
        }
    }
}
