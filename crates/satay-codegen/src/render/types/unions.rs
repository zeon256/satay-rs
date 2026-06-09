use crate::model::UnionVariant;

use super::super::{doc_attrs, ident, rust_type};

pub(super) fn render_union(
    name: &str,
    description: Option<&str>,
    variants: &[UnionVariant],
) -> syn::ItemEnum {
    let name = ident(name);
    let docs = doc_attrs(description);
    let variants = variants
        .iter()
        .map(render_union_variant)
        .collect::<Vec<_>>();

    syn::parse_quote!(
        #(#docs)*
        #[derive(Debug, Clone, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(untagged))]
        pub enum #name {
            #(#variants),*
        }
    )
}

fn render_union_variant(variant: &UnionVariant) -> syn::Variant {
    let name = ident(&variant.rust_name);
    let ty = rust_type(&variant.ty);

    syn::parse_quote!(#name(#ty))
}
