use crate::model::RangeType;

use super::super::{doc_attrs, ident, range_scalar_rust_type};

pub(super) fn render_range_type(range_type: &RangeType) -> Vec<syn::Item> {
    let name = ident(&range_type.rust_name);
    let scalar = range_scalar_rust_type(range_type.scalar);
    let docs = doc_attrs(range_type.description.as_deref());

    vec![
        syn::parse_quote!(
            #(#docs)*
            #[derive(Debug, Clone, PartialEq)]
            #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
            #[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
            pub struct #name {
                pub min: Option<#scalar>,
                pub max: Option<#scalar>,
            }
        ),
        syn::parse_quote!(
            impl #name {
                pub fn as_wire_string(&self) -> String {
                    satay_runtime::format_range(&self.min, &self.max)
                }
            }
        ),
        syn::parse_quote!(
            impl convert::TryFrom<String> for #name {
                type Error = satay_runtime::ParseRangeError;

                fn try_from(value: String) -> Result<Self, Self::Error> {
                    let (min, max) = satay_runtime::parse_range::<#scalar>(&value)?;
                    Ok(Self { min, max })
                }
            }
        ),
        syn::parse_quote!(
            impl From<#name> for String {
                fn from(value: #name) -> Self {
                    value.as_wire_string()
                }
            }
        ),
        syn::parse_quote!(
            impl fmt::Display for #name {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str(&self.as_wire_string())
                }
            }
        ),
    ]
}
