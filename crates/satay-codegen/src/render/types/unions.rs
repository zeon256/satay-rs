use crate::model::{Union, UnionTagStyle, UnionVariant};

use super::super::{doc_attrs, ident, lit_str, rust_type};

pub(super) fn render_union(name: &str, description: Option<&str>, union: &Union) -> syn::ItemEnum {
    let name = ident(name);
    let docs = doc_attrs(description);
    let serde_tag_attr: syn::Attribute = if let Some(tag) = &union.tag
        && tag.style == UnionTagStyle::InternallyTagged
    {
        let property_name = lit_str(&tag.property_name);
        syn::parse_quote!(#[cfg_attr(feature = "serde", serde(tag = #property_name))])
    } else {
        syn::parse_quote!(#[cfg_attr(feature = "serde", serde(untagged))])
    };
    let variants = union
        .variants
        .iter()
        .map(render_union_variant)
        .collect::<Vec<_>>();

    syn::parse_quote!(
        #(#docs)*
        #[derive(Debug, Clone, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #serde_tag_attr
        pub enum #name {
            #(#variants),*
        }
    )
}

fn render_union_variant(variant: &UnionVariant) -> syn::Variant {
    let name = ident(&variant.rust_name);
    let ty = rust_type(&variant.ty);

    if let Some(tag_value) = &variant.tag_value {
        let tag_value = lit_str(tag_value);
        syn::parse_quote!(#[cfg_attr(feature = "serde", serde(rename = #tag_value))] #name(#ty))
    } else {
        syn::parse_quote!(#name(#ty))
    }
}
