use crate::model::{Field, TypeRef};
use syn::parse_quote;

use super::super::{
    doc_attrs, ident, lit_str, parse_as_integer_serde_module, parse_as_string_serde_module,
    rust_field_type,
};

pub fn render_struct(
    name: &str,
    description: Option<&str>,
    fields: &[Field],
    serde: bool,
) -> syn::ItemStruct {
    let name = ident(name);
    let attrs = struct_attrs(description, serde);
    let fields = fields
        .iter()
        .map(|field| render_struct_field(field, serde))
        .collect::<Vec<_>>();

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

fn render_struct_field(field: &Field, serde: bool) -> syn::Field {
    let name = ident(&field.rust_name);
    let ty = rust_field_type(&field.ty, field.required, field.treat_error_as_none);
    let attrs = field_attrs(field, serde);

    parse_quote!(#(#attrs)* pub #name: #ty)
}

fn field_attrs(field: &Field, serde: bool) -> Vec<syn::Attribute> {
    let mut attrs = doc_attrs(field.description.as_deref());
    if !serde {
        return attrs;
    }

    let mut serde_attrs = Vec::new();
    if field.rust_name != field.wire_name {
        let wire_name = lit_str(&field.wire_name);
        serde_attrs.push(quote::quote!(rename = #wire_name));
    }
    if field.treat_error_as_none {
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
