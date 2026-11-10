use quote::quote;

use crate::model::{Enum, EnumFallback, EnumVariant};

use super::super::family::{Family, Variant};
use super::super::{doc_attrs, ident, lit_str};
use super::structs::render_deserialize_impl;

pub(super) fn render_enum(
    family: &Family,
    name: &str,
    description: Option<&str>,
    enum_: &Enum,
) -> Vec<syn::Item> {
    match enum_.fallback {
        EnumFallback::None => render_closed_enum(name, description, enum_),
        EnumFallback::OtherString => render_open_enum(family, name, description, enum_),
    }
}

fn render_closed_enum(name: &str, description: Option<&str>, enum_: &Enum) -> Vec<syn::Item> {
    let name = ident(name);
    let docs = doc_attrs(description);
    let variants = enum_
        .variants
        .iter()
        .map(|variant| render_enum_variant(variant, true));
    let as_str_arms = enum_.variants.iter().map(render_enum_as_str_arm);

    vec![
        syn::parse_quote!(
            #(#docs)*
            #[derive(Debug, Clone, PartialEq, Eq)]
            #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
            pub enum #name {
                #(#variants,)*
            }
        ),
        syn::parse_quote!(
            impl #name {
                pub const fn as_str(&self) -> &'static str {
                    match self {
                        #(#as_str_arms)*
                    }
                }
            }
        ),
        syn::parse_quote!(
            impl AsRef<str> for #name {
                fn as_ref(&self) -> &str {
                    self.as_str()
                }
            }
        ),
        syn::parse_quote!(
            impl fmt::Display for #name {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str(self.as_str())
                }
            }
        ),
    ]
}

/// An enum that keeps unknown wire values in policy text.
fn render_open_enum(
    family: &Family,
    name: &str,
    description: Option<&str>,
    enum_: &Enum,
) -> Vec<syn::Item> {
    let name = ident(name);
    let docs = doc_attrs(description);
    let storage = family.storage();
    let declaration = family.declaration();
    let parameters = family.parameters();
    let decoding_parameters = family.decoding_parameters();
    let arguments = family.arguments();
    let variants = enum_
        .variants
        .iter()
        .map(|variant| render_enum_variant(variant, false));
    let as_str_arms = enum_.variants.iter().map(render_enum_as_str_arm);
    let decode_arms = enum_.variants.iter().map(|variant| -> syn::Arm {
        let variant_name = ident(&variant.rust_name);
        let wire_name = lit_str(&variant.wire_name);
        syn::parse_quote!(#wire_name => Self::#variant_name,)
    });
    let mut shapes = enum_
        .variants
        .iter()
        .map(|variant| Variant {
            name: ident(&variant.rust_name),
            views: vec![],
        })
        .collect::<Vec<_>>();
    shapes.push(Variant {
        name: ident("Other"),
        views: vec![Some(quote!(shape::Text))],
    });

    let mut items: Vec<syn::Item> = vec![syn::parse_quote!(
        #(#docs)*
        pub enum #name<#declaration> {
            #(#variants,)*
            Other(#storage::Text<'s>)
        }
    )];
    items.extend(family.enum_impls(&name, &shapes));
    items.extend([
        syn::parse_quote!(
            impl<#parameters> Eq for #name<#arguments> {}
        ),
        syn::parse_quote!(
            impl<#parameters> #name<#arguments> {
                pub fn as_str(&self) -> &str {
                    match self {
                        #(#as_str_arms)*
                        Self::Other(value) => value.as_ref(),
                    }
                }
            }
        ),
        syn::parse_quote!(
            impl<#parameters> AsRef<str> for #name<#arguments> {
                fn as_ref(&self) -> &str {
                    self.as_str()
                }
            }
        ),
        syn::parse_quote!(
            impl<#parameters> fmt::Display for #name<#arguments> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str(self.as_str())
                }
            }
        ),
        syn::parse_quote!(
            #[cfg(feature = "serde")]
            impl<#parameters> serde::Serialize for #name<#arguments> {
                fn serialize<Serializer>(&self, serializer: Serializer) -> Result<Serializer::Ok, Serializer::Error>
                where
                    Serializer: serde::Serializer,
                {
                    serializer.serialize_str(self.as_str())
                }
            }
        ),
        syn::parse_quote!(
            #[cfg(feature = "serde")]
            impl<'de, #decoding_parameters> shape::DecodeIn<'de, 's, #storage> for #name<#arguments> {
                fn decode_in<__D: serde::Deserializer<'de>>(
                    deserializer: __D,
                    storage: &'s #storage,
                ) -> Result<Self, __D::Error> {
                    shape::text(deserializer, |value| {
                        Ok::<_, #storage::Error>(match value {
                            #(#decode_arms)*
                            other => Self::Other(storage.try_text(other)?),
                        })
                    })
                }
            }
        ),
        render_deserialize_impl(family, &name),
    ]);
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
