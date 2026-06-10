use quote::quote;

use crate::model::{Enum, EnumVariant};

use super::super::{doc_attrs, ident, lit_str};

pub(super) fn render_enum(name: &str, description: Option<&str>, enum_: &Enum) -> Vec<syn::Item> {
    let name = ident(name);
    let docs = doc_attrs(description);
    let variant_defs = enum_
        .variants
        .iter()
        .map(render_enum_variant)
        .collect::<Vec<_>>();
    let unknown_variant = enum_.allow_unknown.then(|| {
        quote!(
            #[default]
            #[cfg_attr(feature = "serde", serde(other))]
            Unknown
        )
    });
    let derive_default = enum_.allow_unknown.then(|| quote!(, Default));
    let as_str_arms = enum_
        .variants
        .iter()
        .map(render_enum_as_str_arm)
        .collect::<Vec<_>>();
    let unknown_as_str_arm = enum_.allow_unknown.then(|| quote!(Self::Unknown => "",));

    let enum_item = syn::parse_quote!(
        #(#docs)*
        #[derive(Debug, Clone, PartialEq, Eq #derive_default)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum #name {
            #(#variant_defs,)*
            #unknown_variant
        }
    );
    let inherent_impl = syn::parse_quote!(
        impl #name {
            pub const fn as_str(&self) -> &'static str {
                match self {
                    #(#as_str_arms)*
                    #unknown_as_str_arm
                }
            }
        }
    );
    let as_ref_impl = syn::parse_quote!(
        impl AsRef<str> for #name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
    );
    let display_impl = syn::parse_quote!(
        impl fmt::Display for #name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    );

    vec![enum_item, inherent_impl, as_ref_impl, display_impl]
}

fn render_enum_variant(variant: &EnumVariant) -> syn::Variant {
    let name = ident(&variant.rust_name);
    if variant.rust_name == variant.wire_name {
        syn::parse_quote!(#name)
    } else {
        let wire_name = lit_str(&variant.wire_name);
        syn::parse_quote!(#[cfg_attr(feature = "serde", serde(rename = #wire_name))] #name)
    }
}

fn render_enum_as_str_arm(variant: &EnumVariant) -> syn::Arm {
    let name = ident(&variant.rust_name);
    let wire_name = lit_str(&variant.wire_name);
    syn::parse_quote!(Self::#name => #wire_name,)
}
