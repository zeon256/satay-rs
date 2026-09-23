use super::{ident, storage::StorageGenerics};
use crate::model::{Operation, ResponseStatus};
use quote::quote;
use syn::{Item, parse_quote};

impl StorageGenerics<'_> {
    pub(super) fn context_decoders(&self, file: &syn::File) -> Vec<Item> {
        self.api.operations.iter().filter(|operation| file.items.iter().any(|item| matches!(item, Item::Fn(item) if item.sig.ident == format!("decode_{}_response", operation.fn_name)))).map(|operation| self.context_decoder(operation)).collect()
    }

    fn context_decoder(&self, operation: &Operation) -> Item {
        let storage = &self.parameter;
        let function = ident(&format!("decode_{}_response_in", operation.fn_name));
        let response = ident(&operation.response_name);
        let response_type = if self.names.contains(&operation.response_name) {
            quote!(#response<'storage, #storage>)
        } else {
            quote!(#response)
        };
        let arms = operation.responses.iter().map(|case| {
            let variant = ident(&case.variant_name);
            let (pattern, status_arg) = match case.status {
                ResponseStatus::Exact(code) => { let code = proc_macro2::Literal::u16_unsuffixed(code); (quote!(#code), quote!()) },
                ResponseStatus::Range(class) => { let low = proc_macro2::Literal::u16_unsuffixed(u16::from(class)*100); let high = proc_macro2::Literal::u16_unsuffixed(u16::from(class)*100+99); (quote!(#low..=#high), quote!(status,)) },
            };
            if let Some(body) = &case.body {
                let seed = self.seed(body);
                let decode = if let Some(projection) = &case.projection {
                    let unwrap = &projection.unwrap_field;
                    let map = match &projection.map_field { Some(value) => quote!(Some(#value)), None => quote!(None) };
                    quote!(satay_runtime::storage_serde::from_projected_json_slice_in(storage, response.body, #unwrap, #map, |context, value| #seed.deserialize(value))?)
                } else {
                    quote!(satay_runtime::storage_serde::from_json_slice_in(storage, response.body, |context, deserializer| #seed.deserialize(deserializer)).map_err(|error| match error {
                        satay_runtime::storage_serde::DecodeError::Storage(error) => satay_runtime::storage_serde::DecodeError::Storage(error),
                        satay_runtime::storage_serde::DecodeError::Decode(error) => satay_runtime::storage_serde::DecodeError::Decode(satay_runtime::Error::from(error)),
                    })?)
                };
                quote!(#pattern => { let value = #decode; Ok(#response::#variant(#status_arg value)) })
            } else if matches!(case.status, ResponseStatus::Range(_)) { quote!(#pattern => Ok(#response::#variant(status))) }
            else { quote!(#pattern => Ok(#response::#variant)) }
        });
        let setup = if operation.responses.iter().any(|case| case.body.is_some()) {
            quote!(
                use serde::de::DeserializeSeed;
            )
        } else {
            quote!(let _ = storage;)
        };
        parse_quote! {
            /// Decodes a buffered response using an explicit storage context.
            pub fn #function<'storage, #storage: satay_runtime::storage_serde::CollectionStorage + 'storage>(storage: &'storage #storage, response: satay_runtime::ResponseParts<&[u8]>) -> Result<#response_type, satay_runtime::storage_serde::DecodeError<#storage::Error, satay_runtime::Error>> {
                #setup
                let status = response.status;
                match status.as_u16() { #(#arms,)* _ => Ok(#response::UnexpectedStatus(status, response.body.to_vec())) }
            }
        }
    }

    pub(super) fn request_expression(&self, operation: &Operation) -> syn::Expr {
        let Some(body) = &operation.request_body else {
            return parse_quote!(satay_runtime::into_empty_request(parts));
        };
        let codec = self.serialization_codec(&body.ty);
        let value = if body.required {
            quote!(satay_runtime::storage_serde::serialize::Encoded::<_, #codec>::new(&body))
        } else {
            quote!(body.as_ref().map(satay_runtime::storage_serde::serialize::Encoded::<_, #codec>::new))
        };
        let call = if body.required {
            quote!(satay_runtime::into_json_request(parts))
        } else {
            quote!(satay_runtime::into_optional_json_request(parts))
        };
        parse_quote!({
            let satay_runtime::RequestParts { method, uri, headers, body } = parts;
            let parts = satay_runtime::RequestParts { method, uri, headers, body: #value };
            #call
        })
    }
}
