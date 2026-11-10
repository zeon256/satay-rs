use quote::quote;

use crate::model::{Union, UnionTagStyle, UnionVariant};

use super::super::family::{Family, Spelling, Variant};
use super::super::{doc_attrs, ident, lit_str};
use super::structs::{compact, render_deserialize_impl};

pub(super) fn render_union(
    family: &Family,
    name: &str,
    description: Option<&str>,
    union: &Union,
) -> Vec<syn::Item> {
    let generic = family.is_generic(name);
    let union_name = ident(name);
    let docs = doc_attrs(description);
    let tag = union
        .tag
        .as_ref()
        .filter(|tag| tag.style == UnionTagStyle::InternallyTagged)
        .map(|tag| lit_str(&tag.property_name));
    let serde_tag_attr: syn::Attribute = match &tag {
        Some(property_name) => {
            syn::parse_quote!(#[cfg_attr(feature = "serde", serde(tag = #property_name))])
        }
        None => syn::parse_quote!(#[cfg_attr(feature = "serde", serde(untagged))]),
    };
    let variants = union
        .variants
        .iter()
        .map(|variant| render_union_variant(family, variant, generic));

    if !generic {
        return vec![syn::parse_quote!(
            #(#docs)*
            #[derive(Debug, Clone, PartialEq)]
            #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
            #serde_tag_attr
            pub enum #union_name {
                #(#variants),*
            }
        )];
    }

    let storage = family.storage();
    let declaration = family.declaration();
    let parameters = family.decoding_parameters();
    let arguments = family.arguments();
    let shapes = union
        .variants
        .iter()
        .map(|variant| Variant {
            name: ident(&variant.rust_name),
            views: vec![family.view(&variant.ty, false)],
        })
        .collect::<Vec<_>>();
    let decode = match &tag {
        Some(property_name) => {
            let tags = union.variants.iter().map(variant_tag).collect::<Vec<_>>();
            let arms = union.variants.iter().zip(&tags).map(|(variant, tag)| {
                let variant_name = ident(&variant.rust_name);
                let shape = family.shape(&variant.ty);
                quote!(#tag => content.decode::<#shape, #storage>(storage).map(Self::#variant_name))
            });
            quote!(
                let (tag, content) = shape::Buffered::tagged(deserializer, #property_name)?;
                match tag.as_str() {
                    #(#arms,)*
                    other => Err(content.unknown_variant(other, &[#(#tags),*])),
                }
            )
        }
        None => {
            let attempts = union.variants.iter().map(|variant| {
                let variant_name = ident(&variant.rust_name);
                let shape = family.shape(&variant.ty);
                quote!(
                    if let Ok(value) = content.decode::<#shape, #storage>(storage) {
                        return Ok(Self::#variant_name(value));
                    }
                )
            });
            let mismatch = lit_str(name);
            quote!(
                let content = shape::Buffered::untagged(deserializer)?;
                #(#attempts)*
                Err(content.mismatch(#mismatch))
            )
        }
    };

    let mut items: Vec<syn::Item> = vec![syn::parse_quote!(
        #(#docs)*
        #[cfg_attr(feature = "serde", derive(serde::Serialize))]
        #[cfg_attr(feature = "serde", serde(bound(serialize = "")))]
        #serde_tag_attr
        pub enum #union_name<#declaration> {
            #(#variants),*
        }
    )];
    items.extend(family.enum_impls(&union_name, &shapes));
    items.push(syn::parse_quote!(
        #[cfg(feature = "serde")]
        impl<'de, #parameters> shape::DecodeIn<'de, 's, #storage> for #union_name<#arguments> {
            fn decode_in<__D: serde::Deserializer<'de>>(
                deserializer: __D,
                storage: &'s #storage,
            ) -> Result<Self, __D::Error> {
                #decode
            }
        }
    ));
    items.push(render_deserialize_impl(family, &union_name));
    items
}

fn variant_tag(variant: &UnionVariant) -> syn::LitStr {
    lit_str(variant.tag_value.as_deref().unwrap_or(&variant.rust_name))
}

fn render_union_variant(family: &Family, variant: &UnionVariant, generic: bool) -> syn::Variant {
    let name = ident(&variant.rust_name);
    let ty = family.ty(&variant.ty, Spelling::Bounded);
    let mut serde_attrs = vec![];
    if let Some(tag_value) = &variant.tag_value {
        let tag_value = lit_str(tag_value);
        serde_attrs.push(quote!(rename = #tag_value));
    }
    if generic && let Some(shape) = family.view(&variant.ty, false) {
        let storage = family.storage();
        let serialize = lit_str(&compact(quote!(shape::serialize::<#shape, #storage, _>)));
        serde_attrs.push(quote!(serialize_with = #serialize));
    }

    if serde_attrs.is_empty() {
        syn::parse_quote!(#name(#ty))
    } else {
        syn::parse_quote!(#[cfg_attr(feature = "serde", serde(#(#serde_attrs),*))] #name(#ty))
    }
}
