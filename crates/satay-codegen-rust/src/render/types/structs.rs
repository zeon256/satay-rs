use std::collections::BTreeSet;

use crate::ident::field_ident;
use crate::model::{BoolStringMapping, CoordinateCodec, CoordinateScalar, Field, TypeRef};
use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::parse_quote;

use super::super::family::{Family, Member, Spelling};
use super::super::{
    doc_attrs, ident, lit_str, parse_as_integer_serde_leaf, parse_as_string_serde_leaf, plain_type,
};

/// How a field crosses the wire when its type alone does not decide it.
enum Codec {
    /// Generated helper functions: coordinates, mapped booleans, and sentinels.
    Helper {
        deserialize: String,
        serialize: String,
    },
    /// Invalid values decode as `None`.
    Lossy,
    /// A runtime serde module for a parsed value.
    Module(syn::LitStr),
    /// The field type's own codec, or its shape for family-dependent types.
    Direct,
}

impl Codec {
    fn new(field: &Field, imports: &mut BTreeSet<String>) -> Self {
        if let Some((deserialize, serialize)) = helper_names(field) {
            Self::Helper {
                deserialize,
                serialize,
            }
        } else if field.treat_error_as_none {
            Self::Lossy
        } else if let Some(module) = parsed_serde_module(field, imports) {
            Self::Module(module)
        } else {
            Self::Direct
        }
    }
}

/// Whether the Rust field is `Option` beyond what its schema type says.
fn is_wrapped(field: &Field) -> bool {
    !field.required || field.treat_error_as_none || !field.none_if.is_empty()
}

/// Whether an absent wire field decodes as `None` instead of an error.
fn has_default(field: &Field) -> bool {
    !field.required || field.treat_error_as_none
}

/// Renders a struct and the implementations its derives cannot provide.
///
/// Family-generic structs derive only `Serialize`: every other trait is written
/// out so it bounds the storage policy by its contract alone, and decoding goes
/// through a storage context.
pub fn render_struct(
    family: &Family,
    name: &str,
    description: Option<&str>,
    fields: &[Field],
    serde: bool,
    imports: &mut BTreeSet<String>,
) -> Vec<syn::Item> {
    let codecs = fields
        .iter()
        .map(|field| {
            if serde {
                Codec::new(field, imports)
            } else {
                Codec::Direct
            }
        })
        .collect::<Vec<_>>();
    let docs = doc_attrs(description);
    let struct_name = ident(name);

    if !family.is_generic(name) {
        let serde_derive = serde.then(|| -> syn::Attribute {
            parse_quote!(
                #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
            )
        });
        let rendered = fields.iter().zip(&codecs).map(|(field, codec)| {
            let attrs = field_attrs(name, field, codec, serde, imports);
            render_struct_field(family, field, attrs)
        });
        let mut items = vec![parse_quote!(
            #(#docs)*
            #[derive(Debug, Clone, PartialEq)]
            #serde_derive
            pub struct #struct_name {
                #(#rendered),*
            }
        )];
        let functions = fields
            .iter()
            .zip(&codecs)
            .filter(|(_, codec)| matches!(codec, Codec::Helper { .. }))
            .flat_map(|(field, _)| render_field_serde_functions(field, imports))
            .collect::<Vec<_>>();
        if !functions.is_empty() {
            items.push(parse_quote!(
                #[cfg(feature = "serde")]
                impl #struct_name {
                    #(#functions)*
                }
            ));
        }
        return items;
    }

    let declaration = family.declaration();
    let parameters = family.parameters();
    let arguments = family.arguments();
    let serde_derive = serde.then(|| {
        quote!(
            #[cfg_attr(feature = "serde", derive(serde::Serialize))]
            #[cfg_attr(feature = "serde", serde(bound(serialize = "")))]
        )
    });
    let rendered = fields.iter().zip(&codecs).map(|(field, codec)| {
        let attrs = generic_field_attrs(family, name, field, codec, serde, imports);
        render_struct_field(family, field, attrs)
    });
    let mut items = vec![parse_quote!(
        #(#docs)*
        #serde_derive
        pub struct #struct_name<#declaration> {
            #(#rendered),*
        }
    )];
    let members = fields
        .iter()
        .map(|field| Member {
            name: ident(&rust_field_name(field)),
            view: family.view(&field.ty, is_wrapped(field)),
        })
        .collect::<Vec<_>>();
    items.extend(family.struct_impls(&struct_name, &members));
    if !serde {
        return items;
    }

    // Deserialization helpers are inlined into the decoder; only serialization
    // is reached through Serde attributes.
    let helpers = fields
        .iter()
        .zip(&codecs)
        .filter(|(_, codec)| matches!(codec, Codec::Helper { .. }))
        .map(|(field, _)| render_field_serde_functions(field, imports))
        .collect::<Vec<_>>();
    if !helpers.is_empty() {
        let serializers = helpers.iter().map(|[_, serialize]| serialize);
        items.push(parse_quote!(
            #[cfg(feature = "serde")]
            impl<#parameters> #struct_name<#arguments> {
                #(#serializers)*
            }
        ));
    }
    let mut deserializers = helpers.into_iter().map(|[deserialize, _]| deserialize);
    let decoders = fields
        .iter()
        .zip(&codecs)
        .enumerate()
        .map(|(index, (field, codec))| {
            let helper = matches!(codec, Codec::Helper { .. })
                .then(|| deserializers.next().expect("one helper per helper field"));
            FieldDecoder::new(family, index, field, codec, helper)
        })
        .collect::<Vec<_>>();
    items.push(render_decode_impl(family, name, &decoders));
    items.push(render_deserialize_impl(family, &struct_name));
    items
}

fn render_struct_field(family: &Family, field: &Field, attrs: Vec<syn::Attribute>) -> syn::Field {
    let name = ident(&rust_field_name(field));
    let ty = family.field_ty(&field.ty, is_wrapped(field), Spelling::Bounded);

    parse_quote!(#(#attrs)* pub #name: #ty)
}

fn serde_attr(parts: Vec<TokenStream>) -> Option<syn::Attribute> {
    (!parts.is_empty()).then(|| parse_quote!(#[cfg_attr(feature = "serde", serde(#(#parts),*))]))
}

fn rename_attr(field: &Field) -> Option<TokenStream> {
    let rust_name = rust_field_name(field);
    let serde_default_name = rust_name.strip_prefix("r#").unwrap_or(&rust_name);
    (serde_default_name != field.wire_name).then(|| {
        let wire_name = lit_str(&field.wire_name);
        quote!(rename = #wire_name)
    })
}

fn field_attrs(
    struct_name: &str,
    field: &Field,
    codec: &Codec,
    serde: bool,
    imports: &mut BTreeSet<String>,
) -> Vec<syn::Attribute> {
    let mut attrs = doc_attrs(field.description.as_deref());
    if !serde {
        return attrs;
    }

    let mut serde_attrs = rename_attr(field).into_iter().collect::<Vec<_>>();
    match codec {
        Codec::Helper {
            deserialize,
            serialize,
        } => {
            let deserialize = lit_str(&format!("{struct_name}::{deserialize}"));
            let serialize = lit_str(&format!("{struct_name}::{serialize}"));
            serde_attrs.push(quote!(deserialize_with = #deserialize));
            serde_attrs.push(quote!(serialize_with = #serialize));
        }
        Codec::Lossy => {
            imports.insert("satay_runtime::treat_error_as_none".to_owned());
            serde_attrs.push(quote!(
                deserialize_with = "treat_error_as_none::deserialize"
            ));
            serde_attrs.push(quote!(serialize_with = "treat_error_as_none::serialize"));
        }
        Codec::Module(module) => serde_attrs.push(quote!(with = #module)),
        Codec::Direct => {}
    }
    if has_default(field) {
        serde_attrs.push(quote!(default));
        serde_attrs.push(quote!(skip_serializing_if = "Option::is_none"));
    }
    attrs.extend(serde_attr(serde_attrs));
    attrs
}

/// Serialization attributes for a family-generic struct, which only derives
/// `Serialize`.
fn generic_field_attrs(
    family: &Family,
    struct_name: &str,
    field: &Field,
    codec: &Codec,
    serde: bool,
    imports: &mut BTreeSet<String>,
) -> Vec<syn::Attribute> {
    let mut attrs = doc_attrs(field.description.as_deref());
    if !serde {
        return attrs;
    }

    let storage = family.storage();
    let view = family
        .view(&field.ty, is_wrapped(field))
        .map(|shape| lit_str(&compact(quote!(shape::serialize::<#shape, #storage, _>))));
    let mut serde_attrs = rename_attr(field).into_iter().collect::<Vec<_>>();
    match (codec, view) {
        (Codec::Helper { serialize, .. }, _) => {
            let serialize = lit_str(&format!("{struct_name}::<'s, {storage}>::{serialize}"));
            serde_attrs.push(quote!(serialize_with = #serialize));
        }
        (Codec::Module(module), _) => {
            let serialize = lit_str(&format!("{}::serialize", module.value()));
            serde_attrs.push(quote!(serialize_with = #serialize));
        }
        (Codec::Lossy | Codec::Direct, Some(view)) => {
            serde_attrs.push(quote!(serialize_with = #view));
        }
        (Codec::Lossy, None) => {
            imports.insert("satay_runtime::treat_error_as_none".to_owned());
            serde_attrs.push(quote!(serialize_with = "treat_error_as_none::serialize"));
        }
        (Codec::Direct, None) => {}
    }
    if has_default(field) {
        serde_attrs.push(quote!(skip_serializing_if = "Option::is_none"));
    }
    attrs.extend(serde_attr(serde_attrs));
    attrs
}

/// Renders tokens as a path string without the spaces `TokenStream` inserts.
pub(in super::super) fn compact(tokens: TokenStream) -> String {
    tokens.to_string().replace(' ', "").replace(",", ", ")
}

/// One field of a generated struct decoder.
struct FieldDecoder {
    name: syn::Ident,
    slot: syn::Ident,
    wire_name: syn::LitStr,
    ty: syn::Type,
    /// A local seed adapting a plain deserialization function.
    seed: Option<syn::ItemImpl>,
    seed_name: syn::Ident,
    /// The expression producing the field from the map's next value.
    value: TokenStream,
    /// Whether an absent field is `None` rather than an error.
    defaults: bool,
}

impl FieldDecoder {
    fn new(
        family: &Family,
        index: usize,
        field: &Field,
        codec: &Codec,
        helper: Option<syn::ImplItemFn>,
    ) -> Self {
        let storage = family.storage();
        let wrapped = is_wrapped(field);
        let ty = family.field_ty(&field.ty, wrapped, Spelling::Bounded);
        // Local names carry a prefix no schema type name can have.
        let seed_name = ident(&format!("__Seed{index}"));
        let adapter = |body: TokenStream| -> syn::ItemImpl {
            parse_quote!(
                impl<'de> de::DeserializeSeed<'de> for #seed_name {
                    type Value = #ty;

                    fn deserialize<__D: serde::Deserializer<'de>>(
                        self,
                        deserializer: __D,
                    ) -> Result<Self::Value, __D::Error> {
                        #body
                    }
                }
            )
        };
        let (seed, value) = match codec {
            Codec::Helper { .. } => {
                let helper = helper.expect("helper fields render deserialization functions");
                let function = &helper.sig.ident;
                (
                    Some(adapter(quote!(#helper #function(deserializer)))),
                    quote!(map.next_value_seed(#seed_name)?),
                )
            }
            Codec::Module(module) => {
                let module = module
                    .parse::<syn::Path>()
                    .expect("runtime serde module is a valid path");
                (
                    Some(adapter(quote!(
                        use #module as codec;
                        codec::deserialize(deserializer)
                    ))),
                    quote!(map.next_value_seed(#seed_name)?),
                )
            }
            Codec::Lossy => {
                let shape = family.shape(field.ty.non_option());
                (
                    None,
                    quote!(map.next_value_seed(shape::lossy::<#shape, #storage>(storage))?),
                )
            }
            Codec::Direct => match family.seed(&field.ty, wrapped) {
                Some(shape) => (
                    None,
                    quote!(map.next_value_seed(shape::seed::<#shape, #storage>(storage))?),
                ),
                None => (None, quote!(map.next_value()?)),
            },
        };
        // Mirrors Serde's derive: a missing field is `None` when defaulted, or
        // when its own `Deserialize` implementation is `Option`'s.
        let defaults =
            has_default(field) || (matches!(codec, Codec::Direct) && field.ty.is_option());

        Self {
            name: ident(&rust_field_name(field)),
            slot: ident(&format!("field_{index}")),
            wire_name: lit_str(&field.wire_name),
            ty,
            seed,
            seed_name,
            value,
            defaults,
        }
    }
}

fn render_decode_impl(family: &Family, name: &str, fields: &[FieldDecoder]) -> syn::Item {
    let storage = family.storage();
    let parameters = family.decoding_parameters();
    let arguments = family.arguments();
    let struct_name = ident(name);
    let expecting = lit_str(&format!("struct {name}"));
    let seeds = fields.iter().filter_map(|field| {
        let seed = field.seed.as_ref()?;
        let seed_name = &field.seed_name;
        Some(quote!(struct #seed_name; #seed))
    });
    let slots = fields.iter().map(|field| {
        let (slot, ty) = (&field.slot, &field.ty);
        quote!(let mut #slot: Option<#ty> = None;)
    });
    let wire_names = fields.iter().map(|field| &field.wire_name);
    let arms = fields.iter().enumerate().map(|(index, field)| {
        let index = Literal::usize_unsuffixed(index);
        let (slot, wire_name, value) = (&field.slot, &field.wire_name, &field.value);
        quote!(
            #index => {
                shape::vacant::<__A::Error, _>(&#slot, #wire_name)?;
                #slot = Some(#value);
            }
        )
    });
    let initializers = fields.iter().map(|field| {
        let (name, slot, wire_name) = (&field.name, &field.slot, &field.wire_name);
        if field.defaults {
            quote!(#name: #slot.unwrap_or_default())
        } else {
            quote!(#name: shape::required::<__A::Error, _>(#slot, #wire_name)?)
        }
    });

    parse_quote!(
        #[cfg(feature = "serde")]
        impl<'de, #parameters> shape::DecodeIn<'de, 's, #storage> for #struct_name<#arguments> {
            fn decode_in<__D: serde::Deserializer<'de>>(
                deserializer: __D,
                storage: &'s #storage,
            ) -> Result<Self, __D::Error> {
                #(#seeds)*

                struct __Visitor<'s, #storage>(&'s #storage);

                impl<'de, #parameters> de::Visitor<'de> for __Visitor<'s, #storage> {
                    type Value = #struct_name<#arguments>;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str(#expecting)
                    }

                    fn visit_map<__A: de::MapAccess<'de>>(
                        self,
                        mut map: __A,
                    ) -> Result<Self::Value, __A::Error> {
                        let storage = self.0;
                        #(#slots)*
                        while let Some(field) =
                            map.next_key_seed(shape::Field(&[#(#wire_names),*]))?
                        {
                            match field {
                                #(#arms)*
                                _ => {
                                    map.next_value::<de::IgnoredAny>()?;
                                }
                            }
                        }
                        Ok(#struct_name { #(#initializers),* })
                    }
                }

                deserializer.deserialize_map(__Visitor(storage))
            }
        }
    )
}

/// Plain `Deserialize` for policies that need no storage context.
pub(in super::super) fn render_deserialize_impl(family: &Family, name: &syn::Ident) -> syn::Item {
    let storage = family.storage();
    let owned = family.owned_decoding_bound();
    let arguments = family.arguments();
    parse_quote!(
        #[cfg(feature = "serde")]
        impl<'de, 's, #storage: #owned> serde::Deserialize<'de> for #name<#arguments> {
            fn deserialize<__D: serde::Deserializer<'de>>(deserializer: __D) -> Result<Self, __D::Error> {
                shape::deserialize::<_, #storage, _>(deserializer)
            }
        }
    )
}

fn helper_names(field: &Field) -> Option<(String, String)> {
    if matches!(field.ty.non_option(), TypeRef::Coordinates(_)) {
        Some((
            coordinates_deserialize_name(field),
            coordinates_serialize_name(field),
        ))
    } else if bool_string_mapping(field).is_some() {
        Some((
            bool_mapping_deserialize_name(field),
            bool_mapping_serialize_name(field),
        ))
    } else if !field.none_if.is_empty() {
        Some((
            none_if_deserialize_name(field),
            none_if_serialize_name(field),
        ))
    } else {
        None
    }
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
    let deserialize_module = coordinates_deserialize_module(field, imports);
    let deserialize: syn::Expr = if field.treat_error_as_none {
        parse_quote!(pair_option::deserialize_lossy(deserializer, #delimiter, #parse))
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
            return Err(Error::custom("coordinate components must be finite"));
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
                use serde::de::Error;

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
                use serde::ser::Error;

                #serialize
            }
        ),
    ]
}

fn coordinates_deserialize_module(field: &Field, imports: &mut BTreeSet<String>) -> syn::Path {
    if !field.required || field.ty.is_option() || field.treat_error_as_none {
        imports.insert("satay_runtime::serde_string::pair::option as pair_option".to_owned());
        parse_quote!(pair_option)
    } else {
        parse_quote!(pair)
    }
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
            parse_quote!(self::#name::try_new(#value).map_err(Error::custom)?)
        }
        CoordinateScalar::F32 | CoordinateScalar::F64 => {
            let ty = coordinate_scalar_type(scalar);
            parse_quote!({
                let value = #component.parse::<#ty>().map_err(Error::custom)?;
                if !value.is_finite() {
                    return Err(Error::custom("coordinate components must be finite"));
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
    let inner_ty = plain_type(field.ty.non_option());
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
