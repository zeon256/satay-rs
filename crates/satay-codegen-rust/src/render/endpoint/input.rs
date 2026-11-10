use std::collections::BTreeSet;

use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::parse_quote;

use crate::model::{Field, Operation, ParameterDefault, TypeRef};

use super::super::family::Family;
use super::super::types::structs;
use super::super::{
    ident, input_builder_arg_type, input_builder_value, input_fields, input_setter_name,
};

pub(super) fn render_input(family: &Family, operation: &Operation) -> Vec<syn::Item> {
    let input_fields = input_fields(operation);

    structs::render_struct(
        family,
        &operation.input_name,
        operation.description.as_deref(),
        &input_fields,
        false,
        // Input structs render without serde attrs, so no imports are collected.
        &mut BTreeSet::new(),
    )
}

/// Generics for implementations on an operation's input.
struct InputGenerics {
    parameters: Option<TokenStream>,
    arguments: Option<TokenStream>,
    /// Bound required to materialize text defaults without a storage context.
    defaults: Option<TokenStream>,
}

impl InputGenerics {
    fn new(family: &Family, operation: &Operation) -> Self {
        if !family.input_is_generic(operation) {
            return Self {
                parameters: None,
                arguments: None,
                defaults: None,
            };
        }
        let (parameters, arguments) = (family.parameters(), family.arguments());
        let storage = family.storage();
        let owned = family.owned_bound();
        Self {
            parameters: Some(quote!(<#parameters>)),
            arguments: Some(quote!(<#arguments>)),
            defaults: has_text_default(operation).then(|| quote!(where #storage: #owned)),
        }
    }
}

/// Whether building the input stores a default in policy text.
pub(in super::super) fn has_text_default(operation: &Operation) -> bool {
    operation.parameters.iter().any(|parameter| {
        !parameter.required
            && !matches!(parameter.ty, TypeRef::Constrained { .. })
            && matches!(
                parameter.default,
                Some(ParameterDefault::String(_) | ParameterDefault::OpenEnum(_))
            )
    })
}

pub(super) fn render_input_impl(family: &Family, operation: &Operation) -> syn::ItemImpl {
    let input_name = ident(&operation.input_name);
    let generics = InputGenerics::new(family, operation);
    let (parameters, arguments, defaults) = (
        &generics.parameters,
        &generics.arguments,
        &generics.defaults,
    );
    let fields = input_fields(operation);
    let required_fields = fields
        .iter()
        .filter(|field| field.required)
        .collect::<Vec<_>>();
    let new_args = required_fields.iter().map(|field| {
        let name = ident(&field.rust_name);
        let ty = input_builder_arg_type(family, &field.ty);
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
        } else if let Some(default) = operation
            .parameters
            .iter()
            .find(|parameter| parameter.rust_name == field.rust_name)
            .and_then(|parameter| parameter.default.as_ref())
        {
            let value = render_parameter_default(family, default, &field.ty, false);
            quote!(#name: Some(#value))
        } else {
            quote!(#name: None)
        }
    });
    let setters = fields
        .iter()
        .filter(|field| !field.required)
        .map(|field| render_input_setter(family, field))
        .collect::<Vec<_>>();

    parse_quote!(
        impl #parameters #input_name #arguments {
            pub fn new(#(#new_args),*) -> Self #defaults {
                Self {
                    #(#initializers),*
                }
            }

            #(#setters)*
        }
    )
}

pub(super) fn render_input_default_impl(
    family: &Family,
    operation: &Operation,
) -> Option<syn::ItemImpl> {
    if input_fields(operation).iter().any(|field| field.required) {
        return None;
    }

    let input_name = ident(&operation.input_name);
    let generics = InputGenerics::new(family, operation);
    let (parameters, arguments, defaults) = (
        &generics.parameters,
        &generics.arguments,
        &generics.defaults,
    );
    Some(parse_quote!(
        impl #parameters Default for #input_name #arguments #defaults {
            fn default() -> Self {
                Self::new()
            }
        }
    ))
}

fn render_parameter_default(
    family: &Family,
    default: &ParameterDefault,
    ty: &TypeRef,
    concrete: bool,
) -> TokenStream {
    if let TypeRef::Constrained { rust_name, inner } = ty {
        let rust_name = ident(rust_name);
        let value = render_parameter_default(family, default, inner, true);
        return quote!(
            #rust_name::try_new(#value)
                .expect("parameter default was validated by satay-codegen")
        );
    }

    let storage = family.storage();
    match (default, ty) {
        (ParameterDefault::String(value), TypeRef::String) if concrete => {
            quote!(String::from(#value))
        }
        (ParameterDefault::String(value), TypeRef::String) => {
            quote!(shape::owned_text::<#storage>(#value))
        }
        (ParameterDefault::Integer(value), TypeRef::Integer(_)) => {
            let value = Literal::i128_unsuffixed(*value);
            quote!(#value)
        }
        (ParameterDefault::F32(value), TypeRef::F32) => {
            let value = Literal::f32_unsuffixed(*value);
            quote!(#value)
        }
        (ParameterDefault::F64(value), TypeRef::F64) => {
            let value = Literal::f64_unsuffixed(*value);
            quote!(#value)
        }
        (ParameterDefault::Bool(value), TypeRef::Bool) => quote!(#value),
        (ParameterDefault::EnumVariant { rust_name, .. }, TypeRef::Named(name)) => {
            let name = ident(name);
            let variant = ident(rust_name);
            quote!(#name::#variant)
        }
        (ParameterDefault::OpenEnum(value), TypeRef::Named(name)) => {
            let name = ident(name);
            quote!(#name::Other(shape::owned_text::<#storage>(#value)))
        }
        _ => unreachable!("validated parameter default must match its lowered type"),
    }
}

fn render_input_setter(family: &Family, field: &Field) -> TokenStream {
    let setter_name = input_setter_name(field);
    let name = ident(&field.rust_name);
    let ty = input_builder_arg_type(family, &field.ty);
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
