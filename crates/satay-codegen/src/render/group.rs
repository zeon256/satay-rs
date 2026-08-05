use syn::{Item, parse_quote};

use crate::ident::type_ident;
use crate::model::{Api, ApiGroup, Field, GroupOperation, Operation, TypeRef};

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
                group,
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
    group: &ApiGroup,
    group_operation: &GroupOperation,
) -> proc_macro2::TokenStream {
    let method = super::ident(&group_operation.method_name);
    let action = super::ident(&format!("{}Action", type_ident(&operation.fn_name)));
    let fields = super::input_fields(operation);
    let required_fields = fields
        .iter()
        .filter(|field| field.required)
        .collect::<Vec<_>>();
    let optional_fields = fields
        .iter()
        .filter(|field| !field.required)
        .collect::<Vec<_>>();
    let docs = render_group_operation_docs(
        operation,
        group,
        group_operation,
        &required_fields,
        &optional_fields,
    );
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

fn render_group_operation_docs(
    operation: &Operation,
    group: &ApiGroup,
    group_operation: &GroupOperation,
    required_fields: &[&Field],
    optional_fields: &[&Field],
) -> Vec<syn::Attribute> {
    let mut sections = operation
        .description
        .as_deref()
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .map(str::to_owned)
        .into_iter()
        .collect::<Vec<_>>();

    if !required_fields.is_empty() {
        let arguments = required_fields
            .iter()
            .map(|field| render_field_doc_item(&format!("`{}`", field.rust_name), field))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("# Arguments\n\n{arguments}"));
    }

    if !optional_fields.is_empty() {
        let action = format!("{}Action", type_ident(&operation.fn_name));
        let links = optional_fields
            .iter()
            .map(|field| {
                let setter = super::input_setter_name(field);
                render_field_doc_item(&format!("[`{setter}`]({action}::{setter})"), field)
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("# Optional request settings\n\n{links}"));

        let example = render_group_operation_example(
            group,
            group_operation,
            required_fields,
            optional_fields[0],
        );
        sections.push(format!("# Example\n\n```rust,ignore\n{example}\n```"));
    }

    let docs = sections.join("\n\n");
    super::doc_attrs(Some(&docs))
}

fn render_field_doc_item(label: &str, field: &Field) -> String {
    let Some(description) = field
        .description
        .as_deref()
        .map(str::trim)
        .filter(|description| !description.is_empty())
    else {
        return format!("- {label}");
    };
    let description = description.replace('\n', "\n  ");
    format!("- {label}: {description}")
}

fn render_group_operation_example(
    group: &ApiGroup,
    group_operation: &GroupOperation,
    required_fields: &[&Field],
    optional_field: &Field,
) -> String {
    let args = required_fields
        .iter()
        .map(|field| field.rust_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let setter = super::input_setter_name(optional_field);

    format!(
        "let request = api\n    .{}()\n    .{}({args})\n    .{}({})\n    .request()?;",
        group.rust_name, group_operation.method_name, setter, optional_field.rust_name,
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
