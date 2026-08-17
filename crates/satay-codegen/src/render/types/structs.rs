use crate::ident::field_ident;
use crate::model::{BoolStringMapping, Field, TypeRef};
use syn::parse_quote;

use super::super::{
    doc_attrs, ident, lit_str, parse_as_integer_serde_module, parse_as_string_serde_module,
    rust_field_type, rust_type,
};

pub fn render_struct(
    name: &str,
    description: Option<&str>,
    fields: &[Field],
    serde: bool,
) -> syn::ItemStruct {
    let attrs = struct_attrs(description, serde);
    let fields = fields
        .iter()
        .map(|field| render_struct_field(name, field, serde))
        .collect::<Vec<_>>();
    let name = ident(name);

    parse_quote!(
        #(#attrs)*
        pub struct #name {
            #(#fields),*
        }
    )
}

fn struct_attrs(description: Option<&str>, serde: bool) -> Vec<syn::Attribute> {
    let mut attrs = doc_attrs(description);
    attrs.push(parse_quote!(#[derive(Debug, Clone, PartialEq)]));
    if serde {
        attrs.push(parse_quote!(
            #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        ));
    }
    attrs
}

fn render_struct_field(struct_name: &str, field: &Field, serde: bool) -> syn::Field {
    let rust_name = rust_field_name(field);
    let name = ident(&rust_name);
    let ty = rust_field_type(
        &field.ty,
        field.required,
        field.treat_error_as_none || !field.none_if.is_empty(),
    );
    let attrs = field_attrs(struct_name, field, serde);

    parse_quote!(#(#attrs)* pub #name: #ty)
}

fn field_attrs(struct_name: &str, field: &Field, serde: bool) -> Vec<syn::Attribute> {
    let mut attrs = doc_attrs(field.description.as_deref());
    if !serde {
        return attrs;
    }

    let mut serde_attrs = vec![];
    let rust_name = rust_field_name(field);
    let serde_default_name = rust_name.strip_prefix("r#").unwrap_or(&rust_name);
    if serde_default_name != field.wire_name {
        let wire_name = lit_str(&field.wire_name);
        serde_attrs.push(quote::quote!(rename = #wire_name));
    }
    if bool_string_mapping(field).is_some() {
        let deserialize = lit_str(&format!(
            "{struct_name}::{}",
            bool_mapping_deserialize_name(field)
        ));
        let serialize = lit_str(&format!(
            "{struct_name}::{}",
            bool_mapping_serialize_name(field)
        ));
        serde_attrs.push(quote::quote!(deserialize_with = #deserialize));
        serde_attrs.push(quote::quote!(serialize_with = #serialize));
    } else if !field.none_if.is_empty() {
        let deserialize = lit_str(&format!(
            "{struct_name}::{}",
            none_if_deserialize_name(field)
        ));
        let serialize = lit_str(&format!("{struct_name}::{}", none_if_serialize_name(field)));
        serde_attrs.push(quote::quote!(deserialize_with = #deserialize));
        serde_attrs.push(quote::quote!(serialize_with = #serialize));
    } else if field.treat_error_as_none {
        serde_attrs.push(quote::quote!(
            deserialize_with = "satay_runtime::treat_error_as_none::deserialize"
        ));
        serde_attrs.push(quote::quote!(
            serialize_with = "satay_runtime::treat_error_as_none::serialize"
        ));
    } else if let Some(module) = parsed_serde_module(field) {
        serde_attrs.push(quote::quote!(with = #module));
    }
    if !field.required || field.treat_error_as_none {
        serde_attrs.push(quote::quote!(default));
        serde_attrs.push(quote::quote!(skip_serializing_if = "Option::is_none"));
    }
    if !serde_attrs.is_empty() {
        attrs.push(parse_quote!(#[cfg_attr(feature = "serde", serde(#(#serde_attrs),*))]));
    }
    attrs
}

pub fn render_field_serde_impl(name: &str, fields: &[Field]) -> Option<syn::ItemImpl> {
    let functions = fields
        .iter()
        .filter(|field| bool_string_mapping(field).is_some() || !field.none_if.is_empty())
        .flat_map(render_field_serde_functions)
        .collect::<Vec<_>>();
    if functions.is_empty() {
        return None;
    }

    let name = ident(name);
    Some(parse_quote!(
        #[cfg(feature = "serde")]
        impl #name {
            #(#functions)*
        }
    ))
}

fn render_field_serde_functions(field: &Field) -> [syn::ImplItemFn; 2] {
    if bool_string_mapping(field).is_some() {
        render_bool_string_mapping_functions(field)
    } else {
        render_none_if_functions(field)
    }
}

fn render_none_if_functions(field: &Field) -> [syn::ImplItemFn; 2] {
    let deserialize_name = ident(&none_if_deserialize_name(field));
    let serialize_name = ident(&none_if_serialize_name(field));
    let inner_ty = rust_type(field.ty.non_option());
    let module = match field.ty.non_option() {
        TypeRef::ParsedString(codec) => parse_as_string_serde_module(codec.parse_as()),
        _ => unreachable!("validated none-if field must use string-backed parse-as"),
    };
    let deserialize_module = if !field.required || field.ty.is_option() {
        format!("{module}::option")
    } else {
        module.to_owned()
    };
    let deserialize_module = syn::parse_str::<syn::Path>(&deserialize_module)
        .expect("runtime serde module path is valid");
    let serialize_module =
        syn::parse_str::<syn::Path>(module).expect("runtime serde module path is valid");
    let none_if = field
        .none_if
        .iter()
        .map(|value| lit_str(value))
        .collect::<Vec<_>>();
    let canonical = none_if
        .first()
        .expect("validated none-if list is non-empty");

    [
        parse_quote!(
            fn #deserialize_name<'de, D>(
                deserializer: D,
            ) -> Result<Option<#inner_ty>, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                #deserialize_module::deserialize_none_if(
                    deserializer,
                    &[#(#none_if),*],
                )
            }
        ),
        parse_quote!(
            #[allow(
                clippy::ref_option,
                clippy::trivially_copy_pass_by_ref,
                reason = "Serde `serialize_with` receives a reference to the field type"
            )]
            fn #serialize_name<S>(
                value: &Option<#inner_ty>,
                serializer: S,
            ) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                #serialize_module::serialize_none_if(value, #canonical, serializer)
            }
        ),
    ]
}

fn render_bool_string_mapping_functions(field: &Field) -> [syn::ImplItemFn; 2] {
    let tokens = BoolMappingTokens::new(field);
    if !field.none_if.is_empty() {
        return render_mapped_bool_none_if_functions(field, &tokens);
    }

    if !field.required || field.ty.is_option() || field.treat_error_as_none {
        render_optional_mapped_bool_functions(&tokens, field.treat_error_as_none)
    } else {
        render_required_mapped_bool_functions(&tokens)
    }
}

struct BoolMappingTokens {
    deserialize_name: syn::Ident,
    serialize_name: syn::Ident,
    true_values: Vec<syn::LitStr>,
    false_values: Vec<syn::LitStr>,
    canonical_true: syn::LitStr,
    canonical_false: syn::LitStr,
    unknown_as: syn::Expr,
}

impl BoolMappingTokens {
    fn new(field: &Field) -> Self {
        let mapping = bool_string_mapping(field).expect("mapped boolean field has a mapping");
        let true_values = mapping
            .true_values
            .iter()
            .map(|value| lit_str(value))
            .collect::<Vec<_>>();
        let false_values = mapping
            .false_values
            .iter()
            .map(|value| lit_str(value))
            .collect::<Vec<_>>();
        let canonical_true = true_values
            .first()
            .expect("validated true-values list is non-empty")
            .clone();
        let canonical_false = false_values
            .first()
            .expect("validated false-values list is non-empty")
            .clone();
        let unknown_as = match mapping.unknown_as {
            Some(value) => parse_quote!(Some(#value)),
            None => parse_quote!(None),
        };

        Self {
            deserialize_name: ident(&bool_mapping_deserialize_name(field)),
            serialize_name: ident(&bool_mapping_serialize_name(field)),
            true_values,
            false_values,
            canonical_true,
            canonical_false,
            unknown_as,
        }
    }
}

fn render_mapped_bool_none_if_functions(
    field: &Field,
    tokens: &BoolMappingTokens,
) -> [syn::ImplItemFn; 2] {
    let BoolMappingTokens {
        deserialize_name,
        serialize_name,
        true_values,
        false_values,
        canonical_true,
        canonical_false,
        unknown_as,
    } = tokens;
    let deserialize_module: syn::Path = if !field.required || field.ty.is_option() {
        parse_quote!(satay_runtime::serde_string::as_bool::option)
    } else {
        parse_quote!(satay_runtime::serde_string::as_bool)
    };
    let none_if = field
        .none_if
        .iter()
        .map(|value| lit_str(value))
        .collect::<Vec<_>>();
    let canonical_none = none_if
        .first()
        .expect("validated none-if list is non-empty");

    [
        parse_quote!(
            fn #deserialize_name<'de, D>(
                deserializer: D,
            ) -> Result<Option<bool>, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                #deserialize_module::deserialize_mapped_none_if(
                    deserializer,
                    &[#(#true_values),*],
                    &[#(#false_values),*],
                    #unknown_as,
                    &[#(#none_if),*],
                )
            }
        ),
        parse_quote!(
            #[allow(
                clippy::ref_option,
                clippy::trivially_copy_pass_by_ref,
                reason = "Serde `serialize_with` receives a reference to the field type"
            )]
            fn #serialize_name<S>(
                value: &Option<bool>,
                serializer: S,
            ) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                satay_runtime::serde_string::as_bool::serialize_mapped_none_if(
                    value,
                    #canonical_true,
                    #canonical_false,
                    #canonical_none,
                    serializer,
                )
            }
        ),
    ]
}

fn render_optional_mapped_bool_functions(
    tokens: &BoolMappingTokens,
    treat_error_as_none: bool,
) -> [syn::ImplItemFn; 2] {
    let BoolMappingTokens {
        deserialize_name,
        serialize_name,
        true_values,
        false_values,
        canonical_true,
        canonical_false,
        unknown_as,
    } = tokens;
    let deserialize: syn::Expr = if treat_error_as_none {
        parse_quote!(
            satay_runtime::serde_string::as_bool::option::deserialize_mapped(
                deserializer,
                &[#(#true_values),*],
                &[#(#false_values),*],
                #unknown_as,
            )
            .or_else(|_| Ok(None))
        )
    } else {
        parse_quote!(
            satay_runtime::serde_string::as_bool::option::deserialize_mapped(
                deserializer,
                &[#(#true_values),*],
                &[#(#false_values),*],
                #unknown_as,
            )
        )
    };

    [
        parse_quote!(
            fn #deserialize_name<'de, D>(
                deserializer: D,
            ) -> Result<Option<bool>, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                #deserialize
            }
        ),
        parse_quote!(
            #[allow(
                clippy::ref_option,
                clippy::trivially_copy_pass_by_ref,
                reason = "Serde `serialize_with` receives a reference to the field type"
            )]
            fn #serialize_name<S>(
                value: &Option<bool>,
                serializer: S,
            ) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                satay_runtime::serde_string::as_bool::option::serialize_mapped(
                    value,
                    #canonical_true,
                    #canonical_false,
                    serializer,
                )
            }
        ),
    ]
}

fn render_required_mapped_bool_functions(tokens: &BoolMappingTokens) -> [syn::ImplItemFn; 2] {
    let BoolMappingTokens {
        deserialize_name,
        serialize_name,
        true_values,
        false_values,
        canonical_true,
        canonical_false,
        unknown_as,
    } = tokens;

    [
        parse_quote!(
            fn #deserialize_name<'de, D>(deserializer: D) -> Result<bool, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                satay_runtime::serde_string::as_bool::deserialize_mapped(
                    deserializer,
                    &[#(#true_values),*],
                    &[#(#false_values),*],
                    #unknown_as,
                )
            }
        ),
        parse_quote!(
            #[allow(
                clippy::trivially_copy_pass_by_ref,
                reason = "Serde `serialize_with` receives a reference to the field type"
            )]
            fn #serialize_name<S>(
                value: &bool,
                serializer: S,
            ) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                satay_runtime::serde_string::as_bool::serialize_mapped(
                    value,
                    #canonical_true,
                    #canonical_false,
                    serializer,
                )
            }
        ),
    ]
}

fn bool_mapping_deserialize_name(field: &Field) -> String {
    let rust_name = rust_field_name(field);
    format!(
        "__satay_deserialize_{}_bool_mapping",
        rust_name.strip_prefix("r#").unwrap_or(&rust_name)
    )
}

fn bool_mapping_serialize_name(field: &Field) -> String {
    let rust_name = rust_field_name(field);
    format!(
        "__satay_serialize_{}_bool_mapping",
        rust_name.strip_prefix("r#").unwrap_or(&rust_name)
    )
}

fn none_if_deserialize_name(field: &Field) -> String {
    let rust_name = rust_field_name(field);
    format!(
        "__satay_deserialize_{}_none_if",
        rust_name.strip_prefix("r#").unwrap_or(&rust_name)
    )
}

fn none_if_serialize_name(field: &Field) -> String {
    let rust_name = rust_field_name(field);
    format!(
        "__satay_serialize_{}_none_if",
        rust_name.strip_prefix("r#").unwrap_or(&rust_name)
    )
}

fn rust_field_name(field: &Field) -> String {
    field.identifier_words.as_ref().map_or_else(
        || field.rust_name.clone(),
        |words| field_ident(&words.join("-")),
    )
}

fn parsed_serde_module(field: &Field) -> Option<syn::LitStr> {
    let module = match field.ty.non_option() {
        TypeRef::ParsedString(codec) => parse_as_string_serde_module(codec.parse_as()),
        TypeRef::ParsedInteger(parse_as) => parse_as_integer_serde_module(*parse_as),
        _ => return None,
    };
    let module = if !field.required || field.ty.is_option() {
        format!("{module}::option")
    } else {
        module.to_owned()
    };
    Some(lit_str(&module))
}
fn bool_string_mapping(field: &Field) -> Option<&BoolStringMapping> {
    match field.ty.non_option() {
        TypeRef::ParsedString(codec) => codec.bool_string_mapping(),
        _ => None,
    }
}
