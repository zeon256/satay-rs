use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Item, parse_quote};

use crate::RootModule;
use crate::ident::{type_ident, unique_ident};
use crate::model::{
    Api, ApiGroup, ApiKeyLocation, ApiKeySecurityScheme, Field, Operation, TypeRef,
};

pub(super) fn render_api_file(api: &Api, root_module: RootModule) -> syn::File {
    let mut items = vec![];
    let has_map_input = api.operations.iter().any(|operation| {
        super::input_fields(operation)
            .iter()
            .any(|field| field.ty.contains_map())
    });

    if has_map_input {
        items.push(Item::Use(parse_quote!(
            use std::collections::BTreeMap;
        )));
    }

    items.extend(
        build_api_group_module_uses(api, root_module)
            .into_iter()
            .map(Item::Use),
    );

    if let Some(operation_use) = build_api_operation_use(api) {
        items.push(Item::Use(operation_use));
    }

    items.extend(
        build_api_operation_module_uses(api)
            .into_iter()
            .map(Item::Use),
    );

    items.push(Item::Struct(render_api_struct(api)));
    items.push(Item::Impl(render_api_default_impl()));
    items.push(Item::Impl(render_api_impl(api)));

    for operation in &api.operations {
        items.push(Item::Struct(render_action_struct(operation)));
        items.push(Item::Impl(render_action_impl(operation)));
        items.push(Item::Impl(render_action_trait_impl(operation)));
    }

    syn::File {
        shebang: None,
        attrs: vec![],
        items,
    }
}

fn build_api_group_module_uses(api: &Api, root_module: RootModule) -> Vec<syn::ItemUse> {
    api.groups
        .iter()
        .map(|group| {
            let module = super::ident(&group.rust_name);
            match root_module {
                RootModule::ModRs => parse_quote!(use super::#module;),
                RootModule::LibRs => parse_quote!(use crate::#module;),
            }
        })
        .collect()
}

fn build_api_operation_use(api: &Api) -> Option<syn::ItemUse> {
    if api.operations.is_empty() {
        return None;
    }

    let mut names = vec![];
    for operation in &api.operations {
        names.push(super::ident(&operation.input_name));
        names.push(super::ident(&operation.response_name));
        for field in super::input_fields(operation) {
            collect_type_refs(&field.ty, &mut names);
        }
    }
    names.sort_by_key(|a| a.to_string());
    names.dedup();

    Some(parse_quote!(use super::{#(#names),*};))
}

fn build_api_operation_module_uses(api: &Api) -> Vec<syn::ItemUse> {
    api.operations
        .iter()
        .map(|operation| {
            let module = super::ident(&operation.fn_name);
            let parts = super::ident(&format!("{}_parts", operation.fn_name));
            let decode = super::ident(&format!("decode_{}_response", operation.fn_name));
            parse_quote!(use super::#module::{#decode, #parts};)
        })
        .collect()
}

fn render_api_struct(api: &Api) -> syn::ItemStruct {
    let auth_fields = api.api_key_security_schemes.iter().map(|scheme| {
        let name = super::ident(&scheme.rust_name);
        quote!(#name: Option<String>)
    });

    parse_quote!(
        #[derive(Debug, Clone)]
        pub struct Api {
            base_url: String,
            #(#auth_fields,)*
        }
    )
}

fn render_api_default_impl() -> syn::ItemImpl {
    parse_quote!(
        impl Default for Api {
            fn default() -> Self {
                Self::new()
            }
        }
    )
}

fn render_api_impl(api: &Api) -> syn::ItemImpl {
    let auth_initializers = api.api_key_security_schemes.iter().map(|scheme| {
        let name = super::ident(&scheme.rust_name);
        quote!(#name: None)
    });
    let auth_setters = api
        .api_key_security_schemes
        .iter()
        .map(render_api_key_setter)
        .collect::<Vec<_>>();
    let group_accessors = api
        .groups
        .iter()
        .map(render_group_accessor)
        .collect::<Vec<_>>();
    let apply_api_keys = render_apply_api_keys_body(api);

    parse_quote!(
        impl Api {
            pub fn new() -> Self {
                Self {
                    base_url: super::SERVER_URL.to_owned(),
                    #(#auth_initializers,)*
                }
            }

            pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
                self.base_url = base_url.into();
                self
            }

            #(#auth_setters)*

            #(#group_accessors)*

            fn apply<B>(&self, parts: &mut satay_runtime::RequestParts<B>) -> Result<(), satay_runtime::Error> {
                #apply_api_keys
                if self.base_url.is_empty() {
                    return Ok(());
                }
                let path_and_query = parts.uri.as_str();
                let base_url = self.base_url.trim_end_matches('/');
                let separator = if path_and_query.starts_with('/') { "" } else { "/" };
                parts.uri = format!("{base_url}{separator}{path_and_query}");
                Ok(())
            }
        }
    )
}

fn render_group_accessor(group: &ApiGroup) -> TokenStream {
    let method = super::ident(&group.rust_name);
    let module = super::ident(&group.rust_name);
    let docs = match &group.wire_name {
        Some(tag) => super::doc_attrs(Some(&format!("Access operations tagged `{tag}`."))),
        None => super::doc_attrs(Some("Access operations without an OpenAPI tag.")),
    };

    quote!(
        #(#docs)*
        pub fn #method(&self) -> #module::Api<'_> {
            #module::Api { api: self }
        }
    )
}

fn render_api_key_setter(scheme: &ApiKeySecurityScheme) -> TokenStream {
    let setter = if scheme.rust_name == "new" || scheme.rust_name == "base_url" {
        super::ident(&format!("with_{}", scheme.rust_name))
    } else {
        super::ident(&scheme.rust_name)
    };
    let field = super::ident(&scheme.rust_name);

    quote!(
        pub fn #setter(mut self, value: impl Into<String>) -> Self {
            self.#field = Some(value.into());
            self
        }
    )
}

fn render_action_constructor(operation: &Operation) -> TokenStream {
    let input = super::ident(&operation.input_name);
    let required_fields = super::input_fields(operation)
        .into_iter()
        .filter(|field| field.required)
        .collect::<Vec<_>>();
    let mut used_arg_names = required_fields
        .iter()
        .map(|field| field.rust_name.clone())
        .collect::<BTreeSet<_>>();
    let root_api_arg_name = unique_ident("api".to_owned(), &mut used_arg_names);
    let root_api_arg = super::ident(&root_api_arg_name);
    let root_api_field = if root_api_arg_name == "api" {
        quote!(api)
    } else {
        quote!(api: #root_api_arg)
    };
    let new_args = required_fields.iter().map(|field| {
        let name = super::ident(&field.rust_name);
        let ty = super::input_builder_arg_type(&field.ty);
        quote!(#name: #ty)
    });
    let new_arg_names = required_fields
        .iter()
        .map(|field| super::ident(&field.rust_name));

    quote!(
        pub(crate) fn new(#root_api_arg: &'a Api #(, #new_args)*) -> Self {
            Self {
                #root_api_field,
                input: #input::new(#(#new_arg_names),*),
            }
        }
    )
}

fn render_apply_api_keys_body(api: &Api) -> TokenStream {
    if api.api_key_security_schemes.is_empty() {
        return quote!();
    }

    let headers = api
        .api_key_security_schemes
        .iter()
        .filter(|scheme| scheme.location == ApiKeyLocation::Header)
        .map(|scheme| {
            let field = super::ident(&scheme.rust_name);
            let wire_name = super::lit_str(&scheme.wire_name);
            quote!(
                if let Some(value) = &self.#field {
                    satay_runtime::insert_header(&mut parts.headers, #wire_name, value)?;
                }
            )
        })
        .collect::<Vec<_>>();
    let query = api
        .api_key_security_schemes
        .iter()
        .filter(|scheme| scheme.location == ApiKeyLocation::Query)
        .map(|scheme| {
            let field = super::ident(&scheme.rust_name);
            let wire_name = super::lit_str(&scheme.wire_name);
            quote!(
                if let Some(value) = &self.#field {
                    satay_runtime::append_query_pair(&mut parts.uri, &mut first_query, #wire_name, value);
                }
            )
        })
        .collect::<Vec<_>>();
    let query_block = if query.is_empty() {
        quote!()
    } else {
        quote!(
            let mut first_query = !parts.uri.contains('?');
            #(#query)*
        )
    };

    quote!(
        #(#headers)*
        #query_block
    )
}

fn render_action_struct(operation: &Operation) -> syn::ItemStruct {
    let action = action_ident(operation);
    let input = super::ident(&operation.input_name);
    let docs = render_action_docs(operation);

    parse_quote!(
        #(#docs)*
        #[must_use = "configure this action and execute it or call `.request()`"]
        #[derive(Debug, Clone)]
        pub struct #action<'a> {
            api: &'a Api,
            input: #input,
        }
    )
}

fn render_action_trait_impl(operation: &Operation) -> syn::ItemImpl {
    let action = action_ident(operation);
    let response = super::ident(&operation.response_name);

    parse_quote!(
        impl satay_runtime::Action for #action<'_> {
            type Response = #response;

            fn request(self) -> Result<http::Request<Vec<u8>>, satay_runtime::Error> {
                self.request()
            }

            fn decode<B: AsRef<[u8]>>(
                response: satay_runtime::ResponseParts<B>,
            ) -> Result<Self::Response, satay_runtime::Error> {
                Self::decode(response)
            }
        }
    )
}

fn render_action_impl(operation: &Operation) -> syn::ItemImpl {
    let action = action_ident(operation);
    let response = super::ident(&operation.response_name);
    let parts_fn = super::ident(&format!("{}_parts", operation.fn_name));
    let decode_fn = super::ident(&format!("decode_{}_response", operation.fn_name));
    let request_expr = super::request_from_parts_expr(operation);
    let setters = super::input_fields(operation)
        .into_iter()
        .filter(|field| !field.required)
        .map(render_action_setter)
        .collect::<Vec<_>>();
    let constructor = render_action_constructor(operation);

    parse_quote!(
        impl<'a> #action<'a> {
            #constructor

            #(#setters)*

            pub fn request(self) -> Result<http::Request<Vec<u8>>, satay_runtime::Error> {
                let api = self.api;
                let mut parts = #parts_fn(self.input)?;
                api.apply(&mut parts)?;
                #request_expr
            }

            pub fn decode<B: AsRef<[u8]>>(
                response: satay_runtime::ResponseParts<B>,
            ) -> Result<#response, satay_runtime::Error> {
                #decode_fn(response)
            }
        }
    )
}

fn render_action_setter(field: Field) -> TokenStream {
    let setter = super::input_setter_name(&field);
    let name = super::ident(&field.rust_name);
    let ty = super::input_builder_arg_type(&field.ty);
    let docs = super::doc_attrs(field.description.as_deref());

    quote!(
        #(#docs)*
        #[must_use = "builder methods return the configured action"]
        pub fn #setter(mut self, #name: #ty) -> Self {
            self.input = self.input.#setter(#name);
            self
        }
    )
}

fn action_ident(operation: &Operation) -> Ident {
    super::ident(&format!("{}Action", type_ident(&operation.fn_name)))
}

fn render_action_docs(operation: &Operation) -> Vec<syn::Attribute> {
    let has_optional_fields = super::input_fields(operation)
        .iter()
        .any(|field| !field.required);
    let mut sections = operation
        .description
        .as_deref()
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .map(str::to_owned)
        .into_iter()
        .collect::<Vec<_>>();

    if has_optional_fields {
        sections.push(
            "Use the chainable methods to configure optional request settings, then call \
             [`Self::request`] or use a transport adapter."
                .to_owned(),
        );
    } else {
        sections.push("Call [`Self::request`] or use a transport adapter.".to_owned());
    }

    let docs = sections.join("\n\n");
    super::doc_attrs(Some(&docs))
}

fn collect_type_refs(ty: &TypeRef, names: &mut Vec<Ident>) {
    match ty {
        TypeRef::Named(name) => {
            names.push(super::ident(name));
        }
        TypeRef::Constrained { rust_name, .. } => {
            names.push(super::ident(rust_name));
        }
        TypeRef::Array(inner) => collect_type_refs(inner, names),
        TypeRef::Map(inner) => collect_type_refs(inner, names),
        TypeRef::Option(inner) => collect_type_refs(inner, names),
        TypeRef::Range(range_type) => {
            names.push(super::ident(&range_type.rust_name));
        }
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
