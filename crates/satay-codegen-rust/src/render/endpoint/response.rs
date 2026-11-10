use syn::parse_quote;

use crate::model::{Operation, ResponseCase, ResponseStatus};

use super::super::family::{Family, Spelling, Variant};
use super::super::{doc_attrs, ident};

pub(super) fn render_response(family: &Family, operation: &Operation) -> Vec<syn::Item> {
    let name = ident(&operation.response_name);
    let docs = doc_attrs(operation.description.as_deref());
    let mut variants = operation
        .responses
        .iter()
        .map(|response| render_response_variant(family, response))
        .collect::<Vec<syn::Variant>>();
    variants.push(parse_quote!(UnexpectedStatus(http::StatusCode, Vec<u8>)));

    if !family.response_is_generic(operation) {
        return vec![parse_quote!(
            #(#docs)*
            #[derive(Debug, Clone, PartialEq)]
            pub enum #name {
                #(#variants),*
            }
        )];
    }

    let declaration = family.declaration();
    let mut shapes = operation
        .responses
        .iter()
        .map(|response| {
            let mut views = vec![];
            if matches!(response.status, ResponseStatus::Range(_)) {
                views.push(None);
            }
            if let Some(body) = &response.body {
                views.push(family.view(body, false));
            }
            Variant {
                name: ident(&response.variant_name),
                views,
            }
        })
        .collect::<Vec<_>>();
    shapes.push(Variant {
        name: ident("UnexpectedStatus"),
        views: vec![None, None],
    });
    let mut items = vec![parse_quote!(
        #(#docs)*
        pub enum #name<#declaration> {
            #(#variants),*
        }
    )];
    items.extend(family.enum_impls(&name, &shapes));
    items
}

fn render_response_variant(family: &Family, response: &ResponseCase) -> syn::Variant {
    let name = ident(&response.variant_name);
    let docs = doc_attrs(response.description.as_deref());
    let body = response
        .body
        .as_ref()
        .map(|body| family.ty(body, Spelling::Bounded));
    if matches!(response.status, ResponseStatus::Range(_)) {
        // Range variants carry the concrete status, mirroring UnexpectedStatus.
        return match body {
            Some(body) => parse_quote!(#(#docs)* #name(http::StatusCode, #body)),
            None => parse_quote!(#(#docs)* #name(http::StatusCode)),
        };
    }
    match body {
        Some(body) => parse_quote!(#(#docs)* #name(#body)),
        None => parse_quote!(#(#docs)* #name),
    }
}
