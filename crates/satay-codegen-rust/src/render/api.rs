use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Item, parse_quote};

use super::endpoint::has_text_default;
use super::family::Family;
use crate::RootModule;
use crate::ident::{type_ident, unique_ident};
use crate::model::{
    Api, ApiGroup, ApiKeyLocation, ApiKeySecurityScheme, Field, Operation, TypeRef,
};

pub(super) fn render_api_file(api: &Api, family: &Family, root_module: RootModule) -> syn::File {
    let mut items = vec![];

    items.extend(
        build_api_group_module_uses(api, root_module)
            .into_iter()
            .map(Item::Use),
    );

    if let Some(operation_use) = build_api_operation_use(api) {
        items.push(Item::Use(operation_use));
    }

    items.extend(
        build_api_operation_module_uses(api, family)
            .into_iter()
            .map(Item::Use),
    );

    items.extend(render_api_items(api, family));

    for operation in &api.operations {
        items.extend(render_action_items(family, operation));
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

fn build_api_operation_module_uses(api: &Api, family: &Family) -> Vec<syn::ItemUse> {
    api.operations
        .iter()
        .map(|operation| {
            let module = super::ident(&operation.fn_name);
            let parts = super::ident(&format!("{}_parts", operation.fn_name));
            let decode = super::ident(&format!("decode_{}_response", operation.fn_name));
            if family.response_is_generic(operation) {
                let decode_in = super::ident(&format!("decode_{}_response_in", operation.fn_name));
                parse_quote!(use super::#module::{#decode, #decode_in, #parts};)
            } else {
                parse_quote!(use super::#module::{#decode, #parts};)
            }
        })
        .collect()
}

/// Renders the root client. Its storage parameter selects the policy for
/// request inputs and for responses decoded without a storage context.
fn render_api_items(api: &Api, family: &Family) -> Vec<syn::Item> {
    let storage = family.storage();
    let fields = api
        .api_key_security_schemes
        .iter()
        .map(|scheme| super::ident(&scheme.rust_name))
        .collect::<Vec<_>>();
    let field_labels = fields
        .iter()
        .map(|field| super::lit_str(&field.to_string()));
    let auth_setters = api
        .api_key_security_schemes
        .iter()
        .map(render_api_key_setter)
        .collect::<Vec<_>>();
    let group_accessors = api
        .groups
        .iter()
        .map(|group| render_group_accessor(family, group))
        .collect::<Vec<_>>();
    let apply_api_keys = render_apply_api_keys_body(api);

    vec![
        parse_quote!(
            pub struct Api<#storage = satay_runtime::AllocStorage> {
                base_url: String,
                #(#fields: Option<String>,)*
                storage: marker::PhantomData<fn() -> #storage>,
            }
        ),
        // Written out so the policy type needs neither `Debug` nor `Clone`.
        parse_quote!(
            impl<#storage> fmt::Debug for Api<#storage> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.debug_struct("Api")
                        .field("base_url", &self.base_url)
                        #(.field(#field_labels, &self.#fields))*
                        .finish()
                }
            }
        ),
        parse_quote!(
            impl<#storage> Clone for Api<#storage> {
                fn clone(&self) -> Self {
                    Self {
                        base_url: self.base_url.clone(),
                        #(#fields: self.#fields.clone(),)*
                        storage: marker::PhantomData,
                    }
                }
            }
        ),
        parse_quote!(
            impl<#storage: satay_runtime::Storage> Default for Api<#storage> {
                fn default() -> Self {
                    Api::new().storage()
                }
            }
        ),
        parse_quote!(
            impl Api<satay_runtime::AllocStorage> {
                pub fn new() -> Self {
                    Self {
                        base_url: super::SERVER_URL.to_owned(),
                        #(#fields: None,)*
                        storage: marker::PhantomData,
                    }
                }
            }
        ),
        parse_quote!(
            impl<#storage: satay_runtime::Storage> Api<#storage> {
                pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
                    self.base_url = base_url.into();
                    self
                }

                #(#auth_setters)*

                #(#group_accessors)*

                /// Selects the storage policy for request inputs and for responses
                /// decoded without a storage context.
                pub fn storage<T: satay_runtime::Storage>(self) -> Api<T> {
                    Api {
                        base_url: self.base_url,
                        #(#fields: self.#fields,)*
                        storage: marker::PhantomData,
                    }
                }

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
        ),
    ]
}

fn render_group_accessor(family: &Family, group: &ApiGroup) -> TokenStream {
    let storage = family.storage();
    let method = super::ident(&group.rust_name);
    let module = super::ident(&group.rust_name);
    let docs = match &group.wire_name {
        Some(tag) => super::doc_attrs(Some(&format!("Access operations tagged `{tag}`."))),
        None => super::doc_attrs(Some("Access operations without an OpenAPI tag.")),
    };

    quote!(
        #(#docs)*
        pub fn #method(&self) -> #module::Api<'_, #storage> {
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

/// How one operation's action spells its generics.
pub(super) struct ActionGenerics {
    /// `'a, 's, S: Storage + 's`, or `'a, S: Storage` when the input is concrete.
    pub parameters: TokenStream,
    /// `'a, 's, S` or `'a, S`.
    pub arguments: TokenStream,
    /// `'_, '_, S` or `'_, S`.
    pub elided: TokenStream,
    /// `<'s, S>` on the input, when it is family-generic.
    pub input_arguments: Option<TokenStream>,
    /// Bound for constructing the input when it stores text defaults.
    pub defaults: Option<TokenStream>,
}

impl ActionGenerics {
    pub fn new(family: &Family, operation: &Operation) -> Self {
        let storage = family.storage();
        if !family.input_is_generic(operation) {
            return Self {
                parameters: quote!('a, #storage: satay_runtime::Storage),
                arguments: quote!('a, #storage),
                elided: quote!('_, #storage),
                input_arguments: None,
                defaults: None,
            };
        }
        let owned = family.owned_bound();
        Self {
            parameters: quote!('a, 's, #storage: satay_runtime::Storage + 's),
            arguments: quote!('a, 's, #storage),
            elided: quote!('_, '_, #storage),
            input_arguments: Some(quote!(<'s, #storage>)),
            defaults: has_text_default(operation).then(|| quote!(where #storage: #owned)),
        }
    }
}

fn render_action_constructor(
    family: &Family,
    operation: &Operation,
    generics: &ActionGenerics,
) -> TokenStream {
    let storage = family.storage();
    let input = super::ident(&operation.input_name);
    let defaults = &generics.defaults;
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
        let ty = super::input_builder_arg_type(family, &field.ty);
        quote!(#name: #ty)
    });
    let new_arg_names = required_fields
        .iter()
        .map(|field| super::ident(&field.rust_name));

    quote!(
        pub(crate) fn new(#root_api_arg: &'a Api<#storage> #(, #new_args)*) -> Self #defaults {
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

fn render_action_items(family: &Family, operation: &Operation) -> Vec<syn::Item> {
    let storage = family.storage();
    let generics = ActionGenerics::new(family, operation);
    let ActionGenerics {
        parameters,
        arguments,
        elided,
        input_arguments,
        ..
    } = &generics;
    let action = action_ident(operation);
    let action_label = super::lit_str(&action.to_string());
    let input = super::ident(&operation.input_name);
    let response = super::ident(&operation.response_name);
    let docs = render_action_docs(operation);
    let parts_fn = super::ident(&format!("{}_parts", operation.fn_name));
    let decode_fn = super::ident(&format!("decode_{}_response", operation.fn_name));
    let decode_in_fn = super::ident(&format!("decode_{}_response_in", operation.fn_name));
    let request_expr = super::request_from_parts_expr(family, operation);
    let setters = super::input_fields(operation)
        .into_iter()
        .filter(|field| !field.required)
        .map(|field| render_action_setter(family, field))
        .collect::<Vec<_>>();
    let constructor = render_action_constructor(family, operation, &generics);
    let response_generic = family.response_is_generic(operation);
    let owned_decoding = family.owned_decoding_bound();

    // Cloning the input rebuilds its policy values, which needs an owned policy.
    let clone_parameters = if input_arguments.is_some() {
        let owned = family.owned_bound();
        quote!('a, 's, #storage: #owned)
    } else {
        parameters.clone()
    };
    let declaration = if input_arguments.is_some() {
        quote!('a, 's, #storage: satay_runtime::Storage + 's = satay_runtime::AllocStorage)
    } else {
        quote!('a, #storage: satay_runtime::Storage = satay_runtime::AllocStorage)
    };
    // Responses decoded without a context live in the client's policy, which
    // must then construct values on its own. Decoding into provided storage
    // accepts any policy, independent of the client's.
    let (decoders, trait_bound, owned_response, storage_response, decode_in_call) =
        if response_generic {
            (
                quote!(
                    pub fn decode<'r>(
                        response: satay_runtime::ResponseParts<&[u8]>,
                    ) -> Result<#response<'r, #storage>, satay_runtime::Error>
                    where
                        #storage: #owned_decoding,
                    {
                        #decode_fn(response)
                    }

                    /// Decodes the response, constructing its values in `storage`.
                    pub fn decode_in<'r, T: satay_runtime::Storage<Error: fmt::Display>>(
                        response: satay_runtime::ResponseParts<&[u8]>,
                        storage: &'r T,
                    ) -> Result<#response<'r, T>, satay_runtime::Error> {
                        #decode_in_fn(response, storage)
                    }
                ),
                owned_decoding.clone(),
                quote!(#response<'static, #storage>),
                quote!(#response<'r, T>),
                quote!(Self::decode_in(response, storage)),
            )
        } else {
            (
                quote!(
                    pub fn decode(
                        response: satay_runtime::ResponseParts<&[u8]>,
                    ) -> Result<#response, satay_runtime::Error> {
                        #decode_fn(response)
                    }
                ),
                quote!(satay_runtime::Storage),
                quote!(#response),
                quote!(#response),
                quote!({
                    let _ = storage;
                    Self::decode(response)
                }),
            )
        };

    vec![
        parse_quote!(
            #(#docs)*
            #[must_use = "configure this action and execute it or call `.request()`"]
            pub struct #action<#declaration> {
                api: &'a Api<#storage>,
                input: #input #input_arguments,
            }
        ),
        parse_quote!(
            impl<#parameters> fmt::Debug for #action<#arguments> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.debug_struct(#action_label)
                        .field("api", &self.api)
                        .field("input", &self.input)
                        .finish()
                }
            }
        ),
        parse_quote!(
            impl<#clone_parameters> Clone for #action<#arguments> {
                fn clone(&self) -> Self {
                    Self {
                        api: self.api,
                        input: self.input.clone(),
                    }
                }
            }
        ),
        parse_quote!(
            impl<#parameters> #action<#arguments> {
                #constructor

                #(#setters)*

                pub fn request(self) -> Result<http::Request<Vec<u8>>, satay_runtime::Error> {
                    let api = self.api;
                    let mut parts = #parts_fn(self.input)?;
                    api.apply(&mut parts)?;
                    #request_expr
                }

                #decoders
            }
        ),
        parse_quote!(
            impl<#storage: #trait_bound> satay_runtime::Action for #action<#elided> {
                type RequestBody = Vec<u8>;
                type Response<'de> = #owned_response;

                fn request(self) -> Result<http::Request<Vec<u8>>, satay_runtime::Error> {
                    self.request()
                }

                fn decode(
                    response: satay_runtime::ResponseParts<&[u8]>,
                ) -> Result<Self::Response<'_>, satay_runtime::Error> {
                    Self::decode(response)
                }
            }
        ),
        parse_quote!(
            impl<#storage: #trait_bound> satay_runtime::OwnedAction for #action<#elided> {
                type OwnedResponse = #owned_response;

                fn decode_owned(
                    response: satay_runtime::ResponseParts<&[u8]>,
                ) -> Result<Self::OwnedResponse, satay_runtime::Error> {
                    Self::decode(response)
                }
            }
        ),
        parse_quote!(
            impl<#storage: #trait_bound> satay_runtime::StorageAction for #action<#elided> {
                type StorageResponse<'r, T: satay_runtime::Storage + 'r> = #storage_response;

                fn decode_in<'r, T: satay_runtime::Storage<Error: fmt::Display>>(
                    response: satay_runtime::ResponseParts<&[u8]>,
                    storage: &'r T,
                ) -> Result<Self::StorageResponse<'r, T>, satay_runtime::Error> {
                    #decode_in_call
                }
            }
        ),
    ]
}

fn render_action_setter(family: &Family, field: Field) -> TokenStream {
    let setter = super::input_setter_name(&field);
    let name = super::ident(&field.rust_name);
    let ty = super::input_builder_arg_type(family, &field.ty);
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
        TypeRef::Coordinates(codec) => {
            names.push(super::ident(codec.target()));
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
