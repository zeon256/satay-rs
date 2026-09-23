//! Context-aware model codecs. Schema types choose seeds before Rust rendering.
use proc_macro2::TokenStream;
use quote::ToTokens;
use quote::quote;
use syn::{Expr, Lit, Meta, Type, punctuated};
use syn::{Item, parse_quote};

use super::{ident, storage::StorageGenerics};
use crate::model::{ComponentKind, EnumFallback, Field, TypeRef, UnionTagStyle};

impl StorageGenerics<'_> {
    pub(super) fn owned_deserializers(&self, file: &mut syn::File, extra: &mut Vec<Item>) {
        let storage = &self.parameter;
        for component in &self.api.components {
            if !self.models.contains(&component.rust_name)
                || matches!(component.kind, ComponentKind::Alias(_))
            {
                continue;
            }
            let name = ident(&component.rust_name);
            let mut found = false;
            for item in &mut file.items {
                let attrs = match item {
                    Item::Struct(item) if item.ident == name => {
                        found = true;
                        &mut item.attrs
                    }
                    Item::Enum(item) if item.ident == name => {
                        found = true;
                        &mut item.attrs
                    }
                    Item::Impl(item)
                        if item.trait_.as_ref().is_some_and(|(_, path, _)| {
                            path.segments.last().unwrap().ident == "Deserialize"
                        }) && matches!(&*item.self_ty, Type::Path(path) if path.path.segments.last().unwrap().ident == name) =>
                    {
                        item.attrs.push(parse_quote!(#[cfg(not(feature = "json"))]));
                        continue;
                    }
                    _ => continue,
                };
                for attr in attrs.iter_mut() {
                    if ToTokens::to_token_stream(attr)
                        .to_string()
                        .contains("derive (serde :: Serialize , serde :: Deserialize)")
                    {
                        *attr =
                            parse_quote!(#[cfg_attr(feature = "serde", derive(serde::Serialize))]);
                    }
                }
                if !matches!(component.kind, ComponentKind::Enum(_)) {
                    attrs.push(parse_quote!(#[cfg_attr(all(feature = "serde", not(feature = "json")), derive(serde::Deserialize))]));
                }
            }
            if found {
                extra.push(parse_quote! {
                    #[cfg(feature = "json")]
                    impl<'storage, 'de, #storage: satay_runtime::StaticStorage + satay_runtime::storage_serde::CollectionStorage> serde::Deserialize<'de> for #name<'storage, #storage> {
                        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                            let context = satay_runtime::storage_serde::DecodeContext::new(#storage::context());
                            let result = <Self as satay_runtime::storage_serde::DeserializeIn<'storage, #storage>>::deserialize_in(&context, deserializer);
                            match context.finish(result) {
                                Ok(value) => Ok(value),
                                Err(satay_runtime::storage_serde::DecodeError::Decode(error)) => Err(error),
                                Err(satay_runtime::storage_serde::DecodeError::Storage(_)) => Err(serde::de::Error::custom("storage construction failed")),
                            }
                        }
                    }
                });
            }
        }
    }

    pub(super) fn seed(&self, ty: &TypeRef) -> TokenStream {
        let ty_rust = self.ty(ty);
        match ty {
            TypeRef::String => quote!(satay_runtime::storage_serde::TextSeed(context)),
            TypeRef::Array(inner) => {
                let element = self.seed(inner);
                quote!(satay_runtime::storage_serde::ContiguousSeed { context, element: #element })
            }
            TypeRef::Option(inner) => {
                let seed = self.seed(inner);
                quote!(satay_runtime::storage_serde::OptionSeed(#seed))
            }
            TypeRef::Map(inner) => {
                let seed = self.seed(inner);
                quote!(satay_runtime::storage_serde::MapSeed(#seed))
            }
            TypeRef::Named(name) if self.models.contains(name) => {
                if let Some(component) = self
                    .api
                    .components
                    .iter()
                    .find(|component| &component.rust_name == name)
                    && let ComponentKind::Alias(inner) = &component.kind
                {
                    return self.seed(inner);
                }
                quote!(satay_runtime::storage_serde::ModelSeed::<_, #ty_rust>::new(context))
            }
            _ => quote!(satay_runtime::storage_serde::ValueSeed::<#ty_rust>::new()),
        }
    }

    pub(super) fn model_seeds(&self, file: &syn::File) -> Vec<Item> {
        let mut result = vec![];
        for component in &self.api.components {
            if !self.models.contains(&component.rust_name) {
                continue;
            }
            let name = ident(&component.rust_name);
            if !file.items.iter().any(|item| match item {
                Item::Struct(item) => item.ident == name,
                Item::Enum(item) => item.ident == name,
                _ => false,
            }) {
                continue;
            }
            let storage = &self.parameter;
            let body = match &component.kind {
                ComponentKind::Struct(fields) => self.struct_seed(&name, fields, file),
                ComponentKind::Enum(value) if value.fallback == EnumFallback::OtherString => {
                    let arms = value.variants.iter().map(|variant| {
                        let wire = &variant.wire_name;
                        let variant = ident(&variant.rust_name);
                        quote!(#wire => Self::#variant,)
                    });
                    quote! {
                        let value = String::deserialize(deserializer)?;
                        Ok(match value.as_str() {
                            #(#arms)*
                            _ => Self::Other(context.storage().try_text(&value).map_err(|error| context.storage_error(error))?),
                        })
                    }
                }
                ComponentKind::Union(union) => {
                    if let Some(tag) = &union.tag
                        && tag.style == UnionTagStyle::InternallyTagged
                    {
                        let tag_name = &tag.property_name;
                        let arms = union.variants.iter().map(|variant| {
                            let tag = variant.tag_value.as_ref().expect("tagged branch");
                            let name = ident(&variant.rust_name); let seed = self.seed(&variant.ty);
                            quote!(#tag => #seed.deserialize(value).map(Self::#name).map_err(serde::de::Error::custom),)
                        });
                        quote! {
                            let value = satay_runtime::JsonValue::deserialize(deserializer)?;
                            let tag = value.get(#tag_name).and_then(|value| value.as_str()).ok_or_else(|| serde::de::Error::custom("missing union discriminator"))?.to_owned();
                            match tag.as_str() { #(#arms)* _ => Err(serde::de::Error::custom("unknown union discriminator")) }
                        }
                    } else {
                        let attempts = union.variants.iter().map(|variant| {
                            let name = ident(&variant.rust_name); let seed = self.seed(&variant.ty);
                            quote! {
                                match (#seed).deserialize(value.clone()) {
                                    Ok(value) => return Ok(Self::#name(value)),
                                    Err(_) if context.has_storage_error() => return Err(serde::de::Error::custom("storage construction failed")),
                                    Err(_) => {}
                                }
                            }
                        });
                        quote! {
                            let value = satay_runtime::JsonValue::deserialize(deserializer)?;
                            #(#attempts)*
                            Err(serde::de::Error::custom("data did not match any union variant"))
                        }
                    }
                }
                _ => continue,
            };
            let imports = match component.kind {
                ComponentKind::Enum(_) => quote!(
                    use serde::Deserialize;
                ),
                ComponentKind::Union(_) => quote!(
                    use serde::{Deserialize, de::DeserializeSeed};
                ),
                _ => quote!(),
            };
            result.push(parse_quote! {
                #[cfg(feature = "json")]
                impl<'storage, #storage: satay_runtime::storage_serde::CollectionStorage + 'storage> satay_runtime::storage_serde::DeserializeIn<'storage, #storage> for #name<'storage, #storage> {
                    #[allow(clippy::too_many_lines)] // One generated visitor arm per schema field.
                    fn deserialize_in<'de, D: serde::Deserializer<'de>>(context: &satay_runtime::storage_serde::DecodeContext<'storage, #storage>, deserializer: D) -> Result<Self, D::Error> {
                        #imports
                        #body
                    }
                }
            });
            result.push(parse_quote! {
                #[cfg(feature = "json")]
                impl<'storage, #storage: satay_runtime::storage_serde::CollectionStorage + 'storage> #name<'storage, #storage> {
                    /// Decodes JSON into the supplied storage context.
                    pub fn from_json_in(storage: &'storage #storage, bytes: &[u8]) -> Result<Self, satay_runtime::storage_serde::DecodeError<#storage::Error, serde_json::Error>> {
                        satay_runtime::storage_serde::from_json_slice_in(storage, bytes, |context, deserializer| <Self as satay_runtime::storage_serde::DeserializeIn<'storage, #storage>>::deserialize_in(context, deserializer))
                    }
                }
            });
        }
        result
    }

    fn struct_seed(&self, name: &syn::Ident, fields: &[Field], file: &syn::File) -> TokenStream {
        let storage = &self.parameter;
        let declarations = fields.iter().map(|field| {
            let name = ident(&field.rust_name);
            quote!(let mut #name = None;)
        });
        let initializers = fields.iter().map(|field| {
            let name = ident(&field.rust_name);
            let wire = &field.wire_name;
            if !field.required || field.treat_error_as_none || field.ty.is_option() {
                quote!(#name: #name.unwrap_or(None))
            } else {
                quote!(#name: #name.ok_or_else(|| serde::de::Error::missing_field(#wire))?)
            }
        });
        let arms = fields.iter().map(|field| {
            let member = ident(&field.rust_name); let wire = &field.wire_name;
            let optional = !field.required && !field.ty.is_option();
            let seed = self.seed(&field.ty);
            let seed = if optional { quote!(satay_runtime::storage_serde::OptionSeed(#seed)) } else { seed };
            let custom = find_deserializer(file, name, &member);
            let decode = if field.treat_error_as_none && matches!(field.ty.non_option(), TypeRef::Named(_) | TypeRef::String | TypeRef::Array(_) | TypeRef::Map(_)) {
                let seed = self.seed(field.ty.non_option());
                quote! {
                    let value = map.next_value::<satay_runtime::JsonValue>()?;
                    satay_runtime::storage_serde::deserialize_lossy(context, #seed, value).map_err(serde::de::Error::custom)?
                }
            } else if let Some(custom) = custom {
                quote! {
                    let value = map.next_value::<satay_runtime::JsonValue>()?;
                    #custom(value).map_err(serde::de::Error::custom)?
                }
            } else { quote!(map.next_value_seed(#seed)?) };
            quote! {
                #wire => {
                    if #member.is_some() { return Err(serde::de::Error::duplicate_field(#wire)); }
                    #member = Some({ #decode });
                }
            }
        });
        quote! {
            struct __SatayVisitor<'context, 'storage, #storage: satay_runtime::storage_serde::CollectionStorage>(&'context satay_runtime::storage_serde::DecodeContext<'storage, #storage>);
            impl<'storage, 'de, #storage: satay_runtime::storage_serde::CollectionStorage> serde::de::Visitor<'de> for __SatayVisitor<'_, 'storage, #storage> {
                type Value = #name<'storage, #storage>;
                fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { formatter.write_str(stringify!(#name)) }
                fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                    let context = self.0;
                    #(#declarations)*
                    while let Some(key) = map.next_key::<String>()? {
                        match key.as_str() { #(#arms)* _ => { map.next_value::<serde::de::IgnoredAny>()?; } }
                    }
                    Ok(#name { #(#initializers),* })
                }
            }
            deserializer.deserialize_map(__SatayVisitor(context))
        }
    }
}

fn find_deserializer(
    file: &syn::File,
    model: &syn::Ident,
    field: &syn::Ident,
) -> Option<syn::ExprPath> {
    let item = file.items.iter().find_map(|item| match item {
        Item::Struct(item) if &item.ident == model => Some(item),
        _ => None,
    })?;
    let field = item
        .fields
        .iter()
        .find(|item| item.ident.as_ref() == Some(field))?;
    for attribute in &field.attrs {
        if !attribute.path().is_ident("cfg_attr") {
            continue;
        }
        let args = attribute
            .parse_args_with(punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
            .ok()?;
        for meta in args {
            let Meta::List(meta) = meta else {
                continue;
            };
            if !meta.path.is_ident("serde") {
                continue;
            }
            let values = meta
                .parse_args_with(
                    punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .ok()?;
            for value in values {
                let Meta::NameValue(value) = value else {
                    continue;
                };
                if !value.path.is_ident("deserialize_with") && !value.path.is_ident("with") {
                    continue;
                }
                let Expr::Lit(value_expr) = value.value else {
                    continue;
                };
                let Lit::Str(text) = value_expr.lit else {
                    continue;
                };
                let path = if value.path.is_ident("with") {
                    format!("{}::deserialize", text.value())
                } else {
                    text.value()
                };
                return syn::parse_str(&path).ok();
            }
        }
    }
    None
}
