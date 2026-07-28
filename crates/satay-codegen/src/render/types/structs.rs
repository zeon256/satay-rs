use crate::model::{Field, TypeRef};
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
    let name = ident(&field.rust_name);
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
    let serde_default_name = field
        .rust_name
        .strip_prefix("r#")
        .unwrap_or(&field.rust_name);
    if serde_default_name != field.wire_name {
        let wire_name = lit_str(&field.wire_name);
        serde_attrs.push(quote::quote!(rename = #wire_name));
    }
    if !field.none_if.is_empty() {
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

pub fn render_none_if_impl(name: &str, fields: &[Field]) -> Option<syn::ItemImpl> {
    let functions = fields
        .iter()
        .filter(|field| !field.none_if.is_empty())
        .flat_map(render_none_if_functions)
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

fn render_none_if_functions(field: &Field) -> [syn::ImplItemFn; 2] {
    let deserialize_name = ident(&none_if_deserialize_name(field));
    let serialize_name = ident(&none_if_serialize_name(field));
    let inner_ty = rust_type(field.ty.non_option());
    let module = match field.ty.non_option() {
        TypeRef::ParsedString(parse_as) => parse_as_string_serde_module(*parse_as),
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
            #[allow(clippy::ref_option)]
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

fn none_if_deserialize_name(field: &Field) -> String {
    format!(
        "__satay_deserialize_{}_none_if",
        field
            .rust_name
            .strip_prefix("r#")
            .unwrap_or(&field.rust_name)
    )
}

fn none_if_serialize_name(field: &Field) -> String {
    format!(
        "__satay_serialize_{}_none_if",
        field
            .rust_name
            .strip_prefix("r#")
            .unwrap_or(&field.rust_name)
    )
}

fn parsed_serde_module(field: &Field) -> Option<syn::LitStr> {
    let module = match field.ty.non_option() {
        TypeRef::ParsedString(parse_as) => parse_as_string_serde_module(*parse_as),
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
