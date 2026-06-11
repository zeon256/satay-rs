use quote::quote;

use crate::model::{Enum, EnumFallback, EnumVariant};

use super::super::{doc_attrs, ident, lit_str};

pub(super) fn render_enum(name: &str, description: Option<&str>, enum_: &Enum) -> Vec<syn::Item> {
    let name = ident(name);
    let docs = doc_attrs(description);
    let variant_defs = enum_
        .variants
        .iter()
        .map(|variant| render_enum_variant(variant, enum_.fallback == EnumFallback::None))
        .collect::<Vec<_>>();
    let fallback_variant = match enum_.fallback {
        EnumFallback::None => None,
        EnumFallback::OtherString => Some(quote!(Other(String))),
    };
    let serde_derive = match enum_.fallback {
        EnumFallback::None => Some(quote!(
            #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        )),
        EnumFallback::OtherString => None,
    };
    let as_str_arms = enum_
        .variants
        .iter()
        .map(render_enum_as_str_arm)
        .collect::<Vec<_>>();

    let enum_item = syn::parse_quote!(
        #(#docs)*
        #[derive(Debug, Clone, PartialEq, Eq)]
        #serde_derive
        pub enum #name {
            #(#variant_defs,)*
            #fallback_variant
        }
    );
    let inherent_impl = match enum_.fallback {
        EnumFallback::None => syn::parse_quote!(
            impl #name {
                pub const fn as_str(&self) -> &'static str {
                    match self {
                        #(#as_str_arms)*
                    }
                }
            }
        ),
        EnumFallback::OtherString => syn::parse_quote!(
            impl #name {
                pub fn as_str(&self) -> &str {
                    match self {
                        #(#as_str_arms)*
                        Self::Other(value) => value.as_str(),
                    }
                }
            }
        ),
    };
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

    let mut items = vec![enum_item, inherent_impl, as_ref_impl, display_impl];
    if enum_.fallback == EnumFallback::OtherString {
        items.push(render_open_enum_serialize_impl(&name));
        items.push(render_open_enum_deserialize_impl(&name, enum_));
    }
    items
}

fn render_enum_variant(variant: &EnumVariant, use_serde_attrs: bool) -> syn::Variant {
    let name = ident(&variant.rust_name);
    if !use_serde_attrs || variant.rust_name == variant.wire_name {
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

fn render_open_enum_serialize_impl(name: &syn::Ident) -> syn::Item {
    syn::parse_quote!(
        #[cfg(feature = "serde")]
        impl serde::Serialize for #name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }
    )
}

fn render_open_enum_deserialize_impl(name: &syn::Ident, enum_: &Enum) -> syn::Item {
    let deserialize_arms = enum_
        .variants
        .iter()
        .map(render_enum_deserialize_arm)
        .collect::<Vec<_>>();

    syn::parse_quote!(
        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for #name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                Ok(match value.as_str() {
                    #(#deserialize_arms)*
                    _ => Self::Other(value),
                })
            }
        }
    )
}

fn render_enum_deserialize_arm(variant: &EnumVariant) -> syn::Arm {
    let name = ident(&variant.rust_name);
    let wire_name = lit_str(&variant.wire_name);
    syn::parse_quote!(#wire_name => Self::#name,)
}
