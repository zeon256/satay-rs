use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::parse_quote;

use crate::model::{ConstrainedType, FloatLimit, IntegerLimit, Validation};

use super::super::{doc_attrs, ident, lit_str, rust_type};

pub(super) fn render_constrained_type(constrained_type: &ConstrainedType) -> syn::ItemStruct {
    let name = ident(&constrained_type.rust_name);
    let inner = rust_type(&constrained_type.inner);
    let validation = render_validation(&constrained_type.validation);
    let derives = nutype_derives(&constrained_type.validation);
    let docs = doc_attrs(constrained_type.description.as_deref());

    parse_quote!(
        #(#docs)*
        #[nutype::nutype(
            #validation,
            derive(#(#derives),*),
            cfg_attr(feature = "serde", derive(Serialize, Deserialize)),
        )]
        pub struct #name(#inner);
    )
}

fn render_validation(validation: &Validation) -> TokenStream {
    match validation {
        Validation::String {
            min_length,
            max_length,
            pattern,
        } => {
            let mut validators = Vec::new();
            if let Some(pattern) = pattern {
                let pattern = lit_str(pattern);
                validators.push(quote!(regex = #pattern));
            }
            if let Some(min_length) = min_length {
                let min_length = Literal::u64_unsuffixed(*min_length);
                validators.push(quote!(len_char_min = #min_length));
            }
            if let Some(max_length) = max_length {
                let max_length = Literal::u64_unsuffixed(*max_length);
                validators.push(quote!(len_char_max = #max_length));
            }
            quote!(validate(#(#validators),*))
        }
        Validation::Integer { minimum, maximum } => {
            let mut validators = Vec::new();
            if let Some(minimum) = minimum {
                validators.push(integer_limit_validator(
                    "greater",
                    "greater_or_equal",
                    *minimum,
                ));
            }
            if let Some(maximum) = maximum {
                validators.push(integer_limit_validator("less", "less_or_equal", *maximum));
            }
            quote!(validate(#(#validators),*))
        }
        Validation::Number { minimum, maximum } => {
            let mut validators = vec![quote!(finite)];
            if let Some(minimum) = minimum {
                validators.push(float_limit_validator(
                    "greater",
                    "greater_or_equal",
                    *minimum,
                ));
            }
            if let Some(maximum) = maximum {
                validators.push(float_limit_validator("less", "less_or_equal", *maximum));
            }
            quote!(validate(#(#validators),*))
        }
        Validation::Array {
            min_items,
            max_items,
        } => {
            let predicate = array_predicate(*min_items, *max_items);
            quote!(validate(#predicate))
        }
    }
}

fn nutype_derives(validation: &Validation) -> Vec<TokenStream> {
    let mut derives = vec![quote!(Debug), quote!(Clone)];

    match validation {
        Validation::String { .. } => {
            derives.extend([
                quote!(PartialEq),
                quote!(Eq),
                quote!(PartialOrd),
                quote!(Ord),
                quote!(AsRef),
                quote!(Deref),
                quote!(TryFrom),
                quote!(Into),
                quote!(Display),
                quote!(Hash),
            ]);
        }
        Validation::Integer { .. } => {
            derives.extend([
                quote!(Copy),
                quote!(PartialEq),
                quote!(Eq),
                quote!(PartialOrd),
                quote!(Ord),
                quote!(AsRef),
                quote!(Deref),
                quote!(TryFrom),
                quote!(Into),
                quote!(Display),
            ]);
        }
        Validation::Number { .. } => {
            derives.extend([
                quote!(Copy),
                quote!(PartialEq),
                quote!(PartialOrd),
                quote!(AsRef),
                quote!(Deref),
                quote!(TryFrom),
                quote!(Into),
                quote!(Display),
            ]);
        }
        Validation::Array { .. } => {
            derives.extend([
                quote!(PartialEq),
                quote!(AsRef),
                quote!(Deref),
                quote!(TryFrom),
                quote!(Into),
            ]);
        }
    }

    derives
}

fn integer_limit_validator(
    exclusive_name: &str,
    inclusive_name: &str,
    limit: IntegerLimit,
) -> TokenStream {
    let name = ident(if limit.exclusive {
        exclusive_name
    } else {
        inclusive_name
    });
    let value = Literal::i128_unsuffixed(limit.value);
    quote!(#name = #value)
}

fn float_limit_validator(
    exclusive_name: &str,
    inclusive_name: &str,
    limit: FloatLimit,
) -> TokenStream {
    let name = ident(if limit.exclusive {
        exclusive_name
    } else {
        inclusive_name
    });
    let value = Literal::f64_unsuffixed(limit.value);
    quote!(#name = #value)
}

fn array_predicate(min_items: Option<u64>, max_items: Option<u64>) -> TokenStream {
    match (min_items, max_items) {
        (Some(min_items), Some(max_items)) => {
            let min_items = Literal::u64_unsuffixed(min_items);
            let max_items = Literal::u64_unsuffixed(max_items);
            quote!(predicate = |items| items.len() >= #min_items && items.len() <= #max_items)
        }
        (Some(min_items), None) => {
            let min_items = Literal::u64_unsuffixed(min_items);
            quote!(predicate = |items| items.len() >= #min_items)
        }
        (None, Some(max_items)) => {
            let max_items = Literal::u64_unsuffixed(max_items);
            quote!(predicate = |items| items.len() <= #max_items)
        }
        (None, None) => unreachable!("array validation requires at least one bound"),
    }
}
