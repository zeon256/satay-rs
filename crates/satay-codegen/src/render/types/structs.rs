use std::collections::BTreeSet;

use crate::ident::field_ident;
use crate::model::{BoolStringMapping, CoordinateCodec, CoordinateScalar, Field, TypeRef};
use syn::parse_quote;

use super::super::{
    doc_attrs, ident, lit_str, parse_as_integer_serde_leaf, parse_as_string_serde_leaf,
    rust_field_type, rust_type,
};

pub fn render_struct(
    name: &str,
    description: Option<&str>,
    fields: &[Field],
    serde: bool,
    imports: &mut BTreeSet<String>,
) -> syn::ItemStruct {
    let attrs = struct_attrs(description, serde);
    let fields = fields
        .iter()
        .map(|field| render_struct_field(name, field, serde, imports))
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

fn render_struct_field(
    struct_name: &str,
    field: &Field,
    serde: bool,
    imports: &mut BTreeSet<String>,
) -> syn::Field {
    let rust_name = rust_field_name(field);
    let name = ident(&rust_name);
    let ty = rust_field_type(
        &field.ty,
        field.required,
        field.treat_error_as_none || !field.none_if.is_empty(),
    );
    let attrs = field_attrs(struct_name, field, serde, imports);

    parse_quote!(#(#attrs)* pub #name: #ty)
}

fn field_attrs(
    struct_name: &str,
    field: &Field,
    serde: bool,
    imports: &mut BTreeSet<String>,
) -> Vec<syn::Attribute> {
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
    if matches!(field.ty.non_option(), TypeRef::Coordinates(_)) {
        let deserialize = lit_str(&format!(
            "{struct_name}::{}",
            coordinates_deserialize_name(field)
        ));
        let serialize = lit_str(&format!(
            "{struct_name}::{}",
            coordinates_serialize_name(field)
        ));
        serde_attrs.push(quote::quote!(deserialize_with = #deserialize));
        serde_attrs.push(quote::quote!(serialize_with = #serialize));
    } else if bool_string_mapping(field).is_some() {
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
        imports.insert("satay_runtime::treat_error_as_none".to_owned());
        serde_attrs.push(quote::quote!(
            deserialize_with = "treat_error_as_none::deserialize"
        ));
        serde_attrs.push(quote::quote!(
            serialize_with = "treat_error_as_none::serialize"
        ));
    } else if let Some(module) = parsed_serde_module(field, imports) {
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

pub fn render_field_serde_impl(
    name: &str,
    fields: &[Field],
    imports: &mut BTreeSet<String>,
) -> Option<syn::ItemImpl> {
    let functions = fields
        .iter()
        .filter(|field| {
            matches!(field.ty.non_option(), TypeRef::Coordinates(_))
                || bool_string_mapping(field).is_some()
                || !field.none_if.is_empty()
        })
        .flat_map(|field| render_field_serde_functions(field, imports))
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

fn render_field_serde_functions(
    field: &Field,
    imports: &mut BTreeSet<String>,
) -> [syn::ImplItemFn; 2] {
    if let TypeRef::Coordinates(codec) = field.ty.non_option() {
        render_coordinates_functions(field, codec, imports)
    } else if bool_string_mapping(field).is_some() {
        render_bool_string_mapping_functions(field, imports)
    } else {
        render_none_if_functions(field, imports)
    }
}

fn render_coordinates_functions(
    field: &Field,
    codec: &CoordinateCodec,
    imports: &mut BTreeSet<String>,
) -> [syn::ImplItemFn; 2] {
    imports.insert("satay_runtime::serde_string::pair".to_owned());
    let deserialize_name = ident(&coordinates_deserialize_name(field));
    let serialize_name = ident(&coordinates_serialize_name(field));
    let target = ident(codec.target());
    let delimiter = lit_str(codec.delimiter());
    let [first, second] = codec.fields();
    let first_name = ident(first.rust_name());
    let second_name = ident(second.rust_name());
    let first_value = parse_coordinate_scalar(first.scalar(), &ident("first"));
    let second_value = parse_coordinate_scalar(second.scalar(), &ident("second"));
    let parse: syn::Expr = parse_quote!(
        |first, second| -> Result<self::#target, D::Error> {
            Ok(self::#target {
                #first_name: #first_value,
                #second_name: #second_value,
            })
        }
    );
    let none_if = field
        .none_if
        .iter()
        .map(|value| lit_str(value))
        .collect::<Vec<_>>();
    let deserialize_module: syn::Path = if !field.required || field.ty.is_option() {
        parse_quote!(pair::option)
    } else {
        parse_quote!(pair)
    };
    let deserialize: syn::Expr = if field.treat_error_as_none {
        parse_quote!(pair::option::deserialize_lossy(deserializer, #delimiter, #parse))
    } else if none_if.is_empty() {
        parse_quote!(#deserialize_module::deserialize(deserializer, #delimiter, #parse))
    } else {
        parse_quote!(
            #deserialize_module::deserialize_none_if(
                deserializer,
                #delimiter,
                &[#(#none_if),*],
                #parse,
            )
        )
    };
    let first_ref = coordinate_scalar_ref(first.scalar(), parse_quote!(&value.#first_name));
    let second_ref = coordinate_scalar_ref(second.scalar(), parse_quote!(&value.#second_name));
    let serialize_value: syn::Expr = parse_quote!({
        let first = #first_ref;
        let second = #second_ref;
        if !first.is_finite() || !second.is_finite() {
            return Err(serde::ser::Error::custom("coordinate components must be finite"));
        }
        pair::serialize(first, second, #delimiter, serializer)
    });
    let optional =
        !field.required || field.ty.is_option() || field.treat_error_as_none || !none_if.is_empty();
    let serialize: syn::Expr = if optional {
        let serialize_none: syn::Expr = match none_if.first() {
            Some(canonical) => parse_quote!(serializer.serialize_str(#canonical)),
            None => parse_quote!(serializer.serialize_none()),
        };
        parse_quote!(
            match value {
                Some(value) => #serialize_value,
                None => #serialize_none,
            }
        )
    } else {
        serialize_value
    };
    // Model names must not resolve to the serde helper's generic parameters.
    let ty: syn::Type = if optional {
        parse_quote!(Option<self::#target>)
    } else {
        parse_quote!(self::#target)
    };

    [
        parse_quote!(
            fn #deserialize_name<'de, D>(deserializer: D) -> Result<#ty, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                #deserialize
            }
        ),
        parse_quote!(
            #[allow(
                clippy::ref_option,
                reason = "Serde `serialize_with` receives a reference to the field type"
            )]
            fn #serialize_name<Serializer>(
                value: &#ty,
                serializer: Serializer,
            ) -> Result<Serializer::Ok, Serializer::Error>
            where
                Serializer: serde::Serializer,
            {
                #serialize
            }
        ),
    ]
}

fn coordinate_scalar_type(scalar: &CoordinateScalar) -> syn::Type {
    match scalar {
        CoordinateScalar::F32 => parse_quote!(f32),
        CoordinateScalar::F64 => parse_quote!(f64),
        CoordinateScalar::Constrained { rust_name, .. } => {
            let name = ident(rust_name);
            parse_quote!(self::#name)
        }
    }
}

fn parse_coordinate_scalar(scalar: &CoordinateScalar, component: &syn::Ident) -> syn::Expr {
    match scalar {
        CoordinateScalar::Constrained { rust_name, inner } => {
            let name = ident(rust_name);
            let value = parse_coordinate_scalar(inner, component);
            parse_quote!(self::#name::try_new(#value).map_err(serde::de::Error::custom)?)
        }
        CoordinateScalar::F32 | CoordinateScalar::F64 => {
            let ty = coordinate_scalar_type(scalar);
            parse_quote!({
                let value = #component.parse::<#ty>().map_err(serde::de::Error::custom)?;
                if !value.is_finite() {
                    return Err(serde::de::Error::custom("coordinate components must be finite"));
                }
                value
            })
        }
    }
}

fn coordinate_scalar_ref(scalar: &CoordinateScalar, value: syn::Expr) -> syn::Expr {
    match scalar {
        CoordinateScalar::Constrained { inner, .. } => {
            let inner_ty = coordinate_scalar_type(inner);
            coordinate_scalar_ref(inner, parse_quote!(AsRef::<#inner_ty>::as_ref(#value)))
        }
        CoordinateScalar::F32 | CoordinateScalar::F64 => value,
    }
}

fn coordinates_deserialize_name(field: &Field) -> String {
    let rust_name = rust_field_name(field);
    format!(
        "__satay_deserialize_{}_coordinates",
        rust_name.strip_prefix("r#").unwrap_or(&rust_name)
    )
}

fn coordinates_serialize_name(field: &Field) -> String {
    let rust_name = rust_field_name(field);
    format!(
        "__satay_serialize_{}_coordinates",
        rust_name.strip_prefix("r#").unwrap_or(&rust_name)
    )
}

fn render_none_if_functions(field: &Field, imports: &mut BTreeSet<String>) -> [syn::ImplItemFn; 2] {
    let deserialize_name = ident(&none_if_deserialize_name(field));
    let serialize_name = ident(&none_if_serialize_name(field));
    let inner_ty = rust_type(field.ty.non_option());
    let leaf = match field.ty.non_option() {
        TypeRef::ParsedString(codec) => parse_as_string_serde_leaf(codec.parse_as()),
        _ => unreachable!("validated none-if field must use string-backed parse-as"),
    };
    imports.insert(format!("satay_runtime::serde_string::{leaf}"));
    let leaf_module = ident(leaf);
    let deserialize_module = if !field.required || field.ty.is_option() {
        imports.insert(format!(
            "satay_runtime::serde_string::{leaf}::option as {leaf}_option"
        ));
        ident(&format!("{leaf}_option"))
    } else {
        leaf_module.clone()
    };
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
            fn #serialize_name<Serializer>(
                value: &Option<#inner_ty>,
                serializer: Serializer,
            ) -> Result<Serializer::Ok, Serializer::Error>
            where
                Serializer: serde::Serializer,
            {
                #leaf_module::serialize_none_if(value, #canonical, serializer)
            }
        ),
    ]
}

fn render_bool_string_mapping_functions(
    field: &Field,
    imports: &mut BTreeSet<String>,
) -> [syn::ImplItemFn; 2] {
    let tokens = BoolMappingTokens::new(field);
    if !field.none_if.is_empty() {
        return render_mapped_bool_none_if_functions(field, &tokens, imports);
    }

    if !field.required || field.ty.is_option() || field.treat_error_as_none {
        render_optional_mapped_bool_functions(&tokens, field.treat_error_as_none, imports)
    } else {
        render_required_mapped_bool_functions(&tokens, imports)
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
            .true_values()
            .iter()
            .map(|value| lit_str(value))
            .collect::<Vec<_>>();
        let false_values = mapping
            .false_values()
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
        let unknown_as = match mapping.unknown_as() {
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
    imports: &mut BTreeSet<String>,
) -> [syn::ImplItemFn; 2] {
    imports.insert("satay_runtime::serde_string::as_bool".to_owned());
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
        imports.insert("satay_runtime::serde_string::as_bool::option as as_bool_option".to_owned());
        parse_quote!(as_bool_option)
    } else {
        parse_quote!(as_bool)
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
            fn #serialize_name<Serializer>(
                value: &Option<bool>,
                serializer: Serializer,
            ) -> Result<Serializer::Ok, Serializer::Error>
            where
                Serializer: serde::Serializer,
            {
                as_bool::serialize_mapped_none_if(
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
    imports: &mut BTreeSet<String>,
) -> [syn::ImplItemFn; 2] {
    imports.insert("satay_runtime::serde_string::as_bool::option as as_bool_option".to_owned());
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
            as_bool_option::deserialize_mapped(
                deserializer,
                &[#(#true_values),*],
                &[#(#false_values),*],
                #unknown_as,
            )
            .or_else(|_| Ok(None))
        )
    } else {
        parse_quote!(
            as_bool_option::deserialize_mapped(
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
            fn #serialize_name<Serializer>(
                value: &Option<bool>,
                serializer: Serializer,
            ) -> Result<Serializer::Ok, Serializer::Error>
            where
                Serializer: serde::Serializer,
            {
                as_bool_option::serialize_mapped(
                    value,
                    #canonical_true,
                    #canonical_false,
                    serializer,
                )
            }
        ),
    ]
}

fn render_required_mapped_bool_functions(
    tokens: &BoolMappingTokens,
    imports: &mut BTreeSet<String>,
) -> [syn::ImplItemFn; 2] {
    imports.insert("satay_runtime::serde_string::as_bool".to_owned());
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
                as_bool::deserialize_mapped(
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
            fn #serialize_name<Serializer>(
                value: &bool,
                serializer: Serializer,
            ) -> Result<Serializer::Ok, Serializer::Error>
            where
                Serializer: serde::Serializer,
            {
                as_bool::serialize_mapped(
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

fn parsed_serde_module(field: &Field, imports: &mut BTreeSet<String>) -> Option<syn::LitStr> {
    let (parent, leaf) = match field.ty.non_option() {
        TypeRef::ParsedString(codec) => {
            ("serde_string", parse_as_string_serde_leaf(codec.parse_as()))
        }
        TypeRef::ParsedInteger(parse_as) => {
            ("serde_integer", parse_as_integer_serde_leaf(*parse_as))
        }
        _ => return None,
    };
    imports.insert(format!("satay_runtime::{parent}"));
    let module = if !field.required || field.ty.is_option() {
        format!("{parent}::{leaf}::option")
    } else {
        format!("{parent}::{leaf}")
    };
    Some(lit_str(&module))
}

fn bool_string_mapping(field: &Field) -> Option<&BoolStringMapping> {
    match field.ty.non_option() {
        TypeRef::ParsedString(codec) => codec.bool_string_mapping(),
        _ => None,
    }
}
