use syn::parse_quote;

use crate::model::{Operation, ResponseCase, ResponseStatus};

use super::super::{doc_attrs, ident, rust_type};

pub(super) fn render_response(operation: &Operation) -> syn::ItemEnum {
    let name = ident(&operation.response_name);
    let docs = doc_attrs(operation.description.as_deref());
    let mut variants = operation
        .responses
        .iter()
        .map(render_response_variant)
        .collect::<Vec<_>>();
    variants.push(parse_quote!(UnexpectedStatus(http::StatusCode, Vec<u8>)));

    parse_quote!(
        #(#docs)*
        #[derive(Debug, Clone, PartialEq)]
        pub enum #name {
            #(#variants),*
        }
    )
}

fn render_response_variant(response: &ResponseCase) -> syn::Variant {
    let name = ident(&response.variant_name);
    let docs = doc_attrs(response.description.as_deref());
    if matches!(response.status, ResponseStatus::Range(_)) {
        // Range variants carry the concrete status, mirroring UnexpectedStatus.
        return match &response.body {
            Some(body) => {
                let body = rust_type(body);
                parse_quote!(#(#docs)* #name(http::StatusCode, #body))
            }
            None => parse_quote!(#(#docs)* #name(http::StatusCode)),
        };
    }
    match &response.body {
        Some(body) => {
            let body = rust_type(body);
            parse_quote!(#(#docs)* #name(#body))
        }
        None => parse_quote!(#(#docs)* #name),
    }
}
