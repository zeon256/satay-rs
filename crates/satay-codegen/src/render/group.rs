use syn::{Item, parse_quote};

use crate::ident::type_ident;
use crate::model::{Api, ApiGroup, GroupOperation, Operation, TypeRef};

pub(super) fn render_group_file(api: &Api, group: &ApiGroup) -> syn::File {
    let mut items = vec![];
    let has_map_input = group.operations.iter().any(|group_operation| {
        let operation = &api.operations[group_operation.operation_index];
        super::input_fields(operation)
            .iter()
            .any(|field| field.required && field.ty.contains_map())
    });
    if has_map_input {
        items.push(Item::Use(parse_quote!(
            use std::collections::BTreeMap;
        )));
    }
    items.push(Item::Use(render_group_use(api, group)));
    items.push(Item::Struct(render_group_struct(group)));
    items.push(Item::Impl(render_group_impl(api, group)));

    syn::File {
        shebang: None,
        attrs: vec![],
        items,
    }
}

fn render_group_use(api: &Api, group: &ApiGroup) -> syn::ItemUse {
    let mut names = vec![];
    for group_operation in &group.operations {
        let operation = &api.operations[group_operation.operation_index];
        names.push(super::ident(&format!(
            "{}Action",
            type_ident(&operation.fn_name)
        )));
        for field in super::input_fields(operation)
            .into_iter()
            .filter(|field| field.required)
        {
            collect_type_refs(&field.ty, &mut names);
        }
    }
    names.sort_by_key(ToString::to_string);
    names.dedup();

    parse_quote!(use super::{Api as RootApi, #(#names),*};)
}

fn render_group_struct(group: &ApiGroup) -> syn::ItemStruct {
    let docs = super::doc_attrs(group.description.as_deref());
    parse_quote!(
        #(#docs)*
        #[derive(Debug, Clone, Copy)]
        pub struct Api<'a> {
            pub(crate) api: &'a RootApi,
        }
    )
}

fn render_group_impl(api: &Api, group: &ApiGroup) -> syn::ItemImpl {
    let methods = group
        .operations
        .iter()
        .map(|group_operation| {
            render_group_operation_method(
                &api.operations[group_operation.operation_index],
                group_operation,
            )
        })
        .collect::<Vec<_>>();

    parse_quote!(
        impl<'a> Api<'a> {
            #(#methods)*
        }
    )
}

fn render_group_operation_method(
    operation: &Operation,
    group_operation: &GroupOperation,
) -> proc_macro2::TokenStream {
    let method = super::ident(&group_operation.method_name);
    let action = super::ident(&format!("{}Action", type_ident(&operation.fn_name)));
    let docs = super::doc_attrs(operation.description.as_deref());
    let required_fields = super::input_fields(operation)
        .into_iter()
        .filter(|field| field.required)
        .collect::<Vec<_>>();
    let args = required_fields.iter().map(|field| {
        let name = super::ident(&field.rust_name);
        let ty = super::input_builder_arg_type(&field.ty);
        quote::quote!(#name: #ty)
    });
    let arg_names = required_fields
        .iter()
        .map(|field| super::ident(&field.rust_name));

    quote::quote!(
        #(#docs)*
        pub fn #method(&self #(, #args)*) -> #action<'a> {
            #action::new(self.api #(, #arg_names)*)
        }
    )
}

fn collect_type_refs(ty: &TypeRef, names: &mut Vec<syn::Ident>) {
    match ty {
        TypeRef::Named(name) => names.push(super::ident(name)),
        TypeRef::Constrained { rust_name, .. } => names.push(super::ident(rust_name)),
        TypeRef::Array(inner) | TypeRef::Map(inner) | TypeRef::Option(inner) => {
            collect_type_refs(inner, names);
        }
        TypeRef::Range(range_type) => names.push(super::ident(&range_type.rust_name)),
        TypeRef::String
        | TypeRef::ParsedString(_)
        | TypeRef::ParsedInteger(_)
        | TypeRef::Integer(_)
        | TypeRef::F32
        | TypeRef::F64
        | TypeRef::Bool
        | TypeRef::JsonValue => {}
    }
}
