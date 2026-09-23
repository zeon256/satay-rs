//! Conditional value traits bound to fields, never to the storage marker.
use quote::{format_ident, quote};
use syn::{Fields, Item, Type, parse_quote};
use syn::{Member, punctuated};

fn derives(attrs: &mut Vec<syn::Attribute>) -> Vec<String> {
    let mut traits = vec![];
    attrs.retain(|attr| {
        if attr.path().is_ident("derive") {
            let paths = attr
                .parse_args_with(
                    punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
                )
                .unwrap();
            traits.extend(
                paths
                    .iter()
                    .map(|path| path.segments.last().unwrap().ident.to_string()),
            );
            false
        } else {
            true
        }
    });
    traits
}

pub(super) fn struct_impls(item: &mut syn::ItemStruct) -> Vec<Item> {
    let traits = derives(&mut item.attrs);
    let fields = item.fields.iter().collect::<Vec<_>>();
    let members = fields
        .iter()
        .enumerate()
        .map(|(index, f)| {
            f.ident
                .clone()
                .map_or_else(|| Member::Unnamed(index.into()), Member::Named)
        })
        .collect::<Vec<_>>();
    let labels = members
        .iter()
        .map(|member| quote!(#member).to_string())
        .collect::<Vec<_>>();
    let name = &item.ident;
    let clone = if traits.iter().any(|name| name == "Copy") {
        quote!(*self)
    } else {
        quote!(Self { #(#members: Clone::clone(&self.#members)),* })
    };
    let debug =
        quote!(f.debug_struct(stringify!(#name))#(.field(#labels, &self.#members))*.finish());
    let eq = conjunction(
        members
            .iter()
            .map(|member| quote!(self.#member == other.#member)),
    );
    implementations(
        &item.ident,
        &item.generics,
        &traits,
        fields.iter().map(|f| f.ty.clone()).collect(),
        clone,
        debug,
        eq,
    )
}

pub(super) fn enum_impls(item: &mut syn::ItemEnum) -> Vec<Item> {
    let traits = derives(&mut item.attrs);
    let mut clones = vec![];
    let mut debugs = vec![];
    let mut equals = vec![];
    let mut types = vec![];
    for variant in &item.variants {
        let name = &variant.ident;
        let values = (0..variant.fields.len())
            .map(|i| format_ident!("v{i}"))
            .collect::<Vec<_>>();
        let others = (0..variant.fields.len())
            .map(|i| format_ident!("other{i}"))
            .collect::<Vec<_>>();
        types.extend(variant.fields.iter().map(|f| f.ty.clone()));
        match &variant.fields {
            Fields::Unit => {
                clones.push(quote!(Self::#name => Self::#name));
                debugs.push(quote!(Self::#name => f.write_str(stringify!(#name))));
                equals.push(quote!((Self::#name, Self::#name) => true));
            }
            Fields::Unnamed(_) => {
                clones.push(
                    quote!(Self::#name(#(#values),*) => Self::#name(#(Clone::clone(#values)),*)),
                );
                debugs.push(quote!(Self::#name(#(#values),*) => f.debug_tuple(stringify!(#name))#(.field(#values))*.finish()));
                let eq = conjunction(
                    values
                        .iter()
                        .zip(&others)
                        .map(|(value, other)| quote!(#value == #other)),
                );
                equals.push(quote!((Self::#name(#(#values),*), Self::#name(#(#others),*)) => #eq));
            }
            Fields::Named(_) => unreachable!("generated storage enums use tuple variants"),
        }
    }
    implementations(
        &item.ident,
        &item.generics,
        &traits,
        types,
        quote!(match self { #(#clones),* }),
        quote!(match self { #(#debugs),* }),
        quote!(match (self, other) { #(#equals,)* _ => false }),
    )
}

#[allow(clippy::too_many_arguments)]
fn implementations(
    name: &syn::Ident,
    generics: &syn::Generics,
    traits: &[String],
    types: Vec<Type>,
    clone: proc_macro2::TokenStream,
    debug: proc_macro2::TokenStream,
    eq: proc_macro2::TokenStream,
) -> Vec<Item> {
    let mut items = vec![];
    for trait_name in traits {
        let (path, body): (syn::Path, proc_macro2::TokenStream) = match trait_name.as_str() {
            "Clone" => (
                parse_quote!(Clone),
                quote!(fn clone(&self) -> Self { #clone }),
            ),
            "Debug" => (
                parse_quote!(core::fmt::Debug),
                quote!(fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { #debug }),
            ),
            "PartialEq" => (
                parse_quote!(PartialEq),
                quote!(fn eq(&self, other: &Self) -> bool { #eq }),
            ),
            "Eq" => (parse_quote!(Eq), quote!()),
            "Copy" => (parse_quote!(Copy), quote!()),
            _ => unreachable!("unexpected generated derive"),
        };
        let mut generics = generics.clone();
        for ty in &types {
            generics
                .make_where_clause()
                .predicates
                .push(parse_quote!(#ty: #path));
        }
        let (implementation, arguments, bounds) = generics.split_for_impl();
        items.push(parse_quote!(impl #implementation #path for #name #arguments #bounds { #body }));
    }
    items
}

pub(super) fn conjunction(
    values: impl IntoIterator<Item = proc_macro2::TokenStream>,
) -> proc_macro2::TokenStream {
    let values = values.into_iter().collect::<Vec<_>>();
    if values.is_empty() {
        quote!(true)
    } else if values.len() == 1 {
        values[0].clone()
    } else {
        quote!(#((#values))&&*)
    }
}
