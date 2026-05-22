use proc_macro2::TokenStream;
use quote::quote;
use syn::parse_quote;

use crate::model::{Field, Operation, TypeRef};

use super::super::types::structs;
use super::super::{
    ident, input_builder_arg_type, input_builder_value, input_fields, input_setter_name,
};

pub(super) fn render_input(operation: &Operation) -> syn::ItemStruct {
    let input_fields = input_fields(operation);

    structs::render_struct(
        &operation.input_name,
        operation.description.as_deref(),
        &input_fields,
        false,
    )
}

pub(super) fn render_input_impl(operation: &Operation) -> syn::ItemImpl {
    let input_name = ident(&operation.input_name);
    let fields = input_fields(operation);
    let required_fields = fields
        .iter()
        .filter(|field| field.required)
        .collect::<Vec<_>>();
    let new_args = required_fields.iter().map(|field| {
        let name = ident(&field.rust_name);
        let ty = input_builder_arg_type(&field.ty);
        quote!(#name: #ty)
    });
    let initializers = fields.iter().map(|field| {
        let name = ident(&field.rust_name);
        if field.required {
            if field.ty == TypeRef::String {
                let value = input_builder_value(quote!(#name), &field.ty);
                quote!(#name: #value)
            } else {
                quote!(#name)
            }
        } else {
            quote!(#name: None)
        }
    });
    let setters = fields
        .iter()
        .filter(|field| !field.required)
        .map(render_input_setter)
        .collect::<Vec<_>>();

    parse_quote!(
        impl #input_name {
            pub fn new(#(#new_args),*) -> Self {
                Self {
                    #(#initializers),*
                }
            }

            #(#setters)*
        }
    )
}

pub(super) fn render_input_default_impl(operation: &Operation) -> Option<syn::ItemImpl> {
    if input_fields(operation).iter().any(|field| field.required) {
        return None;
    }

    let input_name = ident(&operation.input_name);
    Some(parse_quote!(
        impl Default for #input_name {
            fn default() -> Self {
                Self::new()
            }
        }
    ))
}

fn render_input_setter(field: &Field) -> TokenStream {
    let setter_name = input_setter_name(field);
    let name = ident(&field.rust_name);
    let ty = input_builder_arg_type(&field.ty);
    if field.ty.is_option() {
        quote!(
            pub fn #setter_name(mut self, #name: #ty) -> Self {
                self.#name = #name;
                self
            }
        )
    } else {
        let value = input_builder_value(quote!(#name), &field.ty);
        quote!(
            pub fn #setter_name(mut self, #name: #ty) -> Self {
                self.#name = Some(#value);
                self
            }
        )
    }
}
