use proc_macro2::TokenStream;
use quote::quote;
use syn::parse_quote;

use super::super::family::{Family, Spelling};
use super::super::{doc_attrs, ident, request_from_parts_expr};
use crate::model::{Operation, ResponseCase, ResponseStatus};

pub(super) fn render_encode_function(family: &Family, operation: &Operation) -> syn::ItemFn {
    let docs = doc_attrs(operation.description.as_deref());
    let parts_fn = ident(&format!("{}_parts", operation.fn_name));
    let encode_fn = ident(&format!("encode_{}", operation.fn_name));
    let input_type = ident(&operation.input_name);
    let encode_expr = request_from_parts_expr(family, operation);
    let (parameters, arguments) = if family.input_is_generic(operation) {
        let (parameters, arguments) = (family.parameters(), family.arguments());
        (Some(quote!(<#parameters>)), Some(quote!(<#arguments>)))
    } else {
        (None, None)
    };

    parse_quote!(
        #(#docs)*
        pub fn #encode_fn #parameters(
            input: #input_type #arguments,
        ) -> Result<http::Request<Vec<u8>>, satay_runtime::Error> {
            let parts = #parts_fn(input)?;
            #encode_expr
        }
    )
}

/// Renders the response decoders.
///
/// A family-generic response decodes into a caller-provided storage context;
/// the context-free decoder delegates to it for owned policies.
pub(super) fn render_decode_functions(family: &Family, operation: &Operation) -> Vec<syn::ItemFn> {
    let decode_fn = ident(&format!("decode_{}_response", operation.fn_name));
    let response_name = ident(&operation.response_name);
    let generic = family.response_is_generic(operation);
    let arms = operation
        .responses
        .iter()
        .map(|response| render_decode_arm(family, response, &response_name));
    let body: TokenStream = quote!(
        let status = response.status;
        match status.as_u16() {
            #(#arms)*
            _ => {
                let body = response.body;
                Ok(#response_name::UnexpectedStatus(status, body.to_vec()))
            }
        }
    );

    if !generic {
        return vec![parse_quote!(
            pub fn #decode_fn(
                response: satay_runtime::ResponseParts<&[u8]>,
            ) -> Result<#response_name, satay_runtime::Error> {
                #body
            }
        )];
    }

    let decode_in_fn = ident(&format!("decode_{}_response_in", operation.fn_name));
    let storage = family.storage();
    let parameters = family.decoding_parameters();
    let arguments = family.arguments();
    let owned = family.owned_decoding_bound();
    vec![
        parse_quote!(
            /// Decodes the response for a storage policy that needs no context.
            pub fn #decode_fn<'s, #storage: #owned>(
                response: satay_runtime::ResponseParts<&[u8]>,
            ) -> Result<#response_name<#arguments>, satay_runtime::Error> {
                #decode_in_fn(response, #storage::CONTEXT)
            }
        ),
        parse_quote!(
            /// Decodes the response, constructing its values in `storage`.
            pub fn #decode_in_fn<#parameters>(
                response: satay_runtime::ResponseParts<&[u8]>,
                storage: &'s #storage,
            ) -> Result<#response_name<#arguments>, satay_runtime::Error> {
                #body
            }
        ),
    ]
}

fn render_decode_arm(
    family: &Family,
    response: &ResponseCase,
    response_name: &syn::Ident,
) -> syn::Arm {
    let variant = ident(&response.variant_name);
    let decode = response
        .body
        .as_ref()
        .map(|_| render_decode_body(family, response));
    match response.status {
        ResponseStatus::Exact(code) => {
            let status = proc_macro2::Literal::u16_unsuffixed(code);
            match decode {
                Some(decode) => parse_quote!(
                    #status => {
                        let body = response.body;
                        let value = #decode;
                        Ok(#response_name::#variant(value))
                    }
                ),
                None => parse_quote!(
                    #status => {
                        Ok(#response_name::#variant)
                    }
                ),
            }
        }
        ResponseStatus::Range(class) => {
            let lo = u16::from(class) * 100;
            let hi = lo + 99;
            let lo = proc_macro2::Literal::u16_unsuffixed(lo);
            let hi = proc_macro2::Literal::u16_unsuffixed(hi);
            match decode {
                Some(decode) => parse_quote!(
                    #lo..=#hi => {
                        let body = response.body;
                        let value = #decode;
                        Ok(#response_name::#variant(status, value))
                    }
                ),
                None => parse_quote!(
                    #lo..=#hi => {
                        Ok(#response_name::#variant(status))
                    }
                ),
            }
        }
    }
}

fn render_decode_body(family: &Family, response: &ResponseCase) -> syn::Expr {
    let body = response
        .body
        .as_ref()
        .expect("decoded responses have a body");
    let projection = response.projection.as_ref().map(|projection| {
        let unwrap_field = &projection.unwrap_field;
        let map_field: syn::Expr = match projection.map_field.as_ref() {
            Some(map_field) => parse_quote!(Some(#map_field)),
            None => parse_quote!(None),
        };
        quote!(#unwrap_field, #map_field)
    });

    if let Some(shape) = family.seed(body, false) {
        let storage = family.storage();
        return match projection {
            Some(projection) => parse_quote!(
                shape::from_projected_json_slice::<#shape, #storage>(body, #projection, storage)?
            ),
            None => parse_quote!(shape::from_json_slice::<#shape, #storage>(body, storage)?),
        };
    }
    let body_type = family.ty(body, Spelling::Bounded);
    match projection {
        Some(projection) => parse_quote!(
            satay_runtime::from_projected_json_slice::<#body_type>(body, #projection)?
        ),
        None => parse_quote!(satay_runtime::from_json_slice::<#body_type>(body)?),
    }
}
