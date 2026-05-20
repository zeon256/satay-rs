use syn::parse_quote;

use super::super::{doc_attrs, ident, request_from_parts_expr, rust_type};
use crate::model::{Operation, ResponseCase};

pub(super) fn render_encode_function(operation: &Operation) -> syn::ItemFn {
    let docs = doc_attrs(operation.description.as_deref());
    let parts_fn = ident(&format!("{}_parts", operation.fn_name));
    let encode_fn = ident(&format!("encode_{}", operation.fn_name));
    let input_type = ident(&operation.input_name);
    let encode_expr = request_from_parts_expr(operation);

    parse_quote!(
        #(#docs)*
        pub fn #encode_fn(input: #input_type) -> Result<http::Request<Vec<u8>>, satay_runtime::Error> {
            let parts = #parts_fn(input)?;
            #encode_expr
        }
    )
}

pub(super) fn render_decode_function(operation: &Operation) -> syn::ItemFn {
    let decode_fn = ident(&format!("decode_{}_response", operation.fn_name));
    let response_name = ident(&operation.response_name);
    let arms = operation
        .responses
        .iter()
        .map(|response| render_decode_arm(response, &response_name))
        .collect::<Vec<_>>();

    parse_quote!(
        pub fn #decode_fn<B: AsRef<[u8]>>(
            response: satay_runtime::ResponseParts<B>,
        ) -> Result<#response_name, satay_runtime::Error> {
            let status = response.status;
            match status.as_u16() {
                #(#arms)*
                _ => {
                    let body = response.body;
                    Ok(#response_name::UnexpectedStatus(status, body.as_ref().to_vec()))
                }
            }
        }
    )
}

fn render_decode_arm(response: &ResponseCase, response_name: &syn::Ident) -> syn::Arm {
    let status = proc_macro2::Literal::u16_unsuffixed(response.status);
    let variant = ident(&response.variant_name);
    match &response.body {
        Some(body) => {
            let body = rust_type(body);
            parse_quote!(
                #status => {
                    let body = response.body;
                    let value = satay_runtime::from_json_slice::<#body>(body.as_ref())?;
                    Ok(#response_name::#variant(value))
                }
            )
        }
        None => parse_quote!(
            #status => {
                Ok(#response_name::#variant)
            }
        ),
    }
}
