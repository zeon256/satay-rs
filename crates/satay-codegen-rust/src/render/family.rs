//! Decides which generated items carry the storage family and how they spell it.
//!
//! An item is family-generic when its schema reaches text, an array, or a map.
//! Such an item takes `<'s, S: satay_runtime::Storage + 's>` and stores the
//! policy's `Text`, `Contiguous`, and `Map` projections directly, so the default
//! policy keeps `String`, `Vec<T>`, and `BTreeMap<String, V>` fields.
//!
//! The projections promise only read access and contextual construction. The
//! behavior a model needs on top of that comes from `satay_runtime::shape`: a
//! shape marker mirrors a field's type and selects the formatting, equality,
//! cloning, and codec implementation for it. Eligibility follows the schema
//! graph, not Rust type spellings: constrained newtypes, parsed values, closed
//! enums, `JsonValue`, and API configuration keep concrete representations.

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt;
use syn::parse_quote;

use super::{ident, input_fields, lit_str, plain_type};
use crate::model::{Api, ComponentKind, EnumFallback, Operation, TypeRef};

pub(crate) struct Family {
    parameter: syn::Ident,
    generic: BTreeSet<String>,
    aliases: BTreeMap<String, TypeRef>,
}

/// How family-dependent types are written at a use site.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Spelling {
    /// `S::Text<'s>`, inside an item that bounds `S: Storage`.
    Bounded,
    /// `<S as satay_runtime::Storage>::Text<'s>`, where bounds cannot be declared.
    Qualified,
    /// `String`, `Vec<T>`, and `BTreeMap<String, V>`: the default policy, spelled
    /// out for items that are not family-generic.
    Concrete,
}

impl Family {
    pub fn new(api: &Api) -> Self {
        let mut family = Self {
            parameter: storage_parameter(api),
            generic: BTreeSet::new(),
            aliases: api
                .components
                .iter()
                .filter_map(|component| match &component.kind {
                    ComponentKind::Alias(ty) => Some((component.rust_name.clone(), ty.clone())),
                    _ => None,
                })
                .collect(),
        };
        loop {
            let previous = family.generic.len();
            for component in &api.components {
                let generic = match &component.kind {
                    ComponentKind::Struct(fields) => fields.iter().any(|f| family.uses(&f.ty)),
                    ComponentKind::Alias(ty) => family.uses(ty),
                    ComponentKind::Union(union) => {
                        union.variants.iter().any(|v| family.uses(&v.ty))
                    }
                    ComponentKind::Enum(value) => value.fallback == EnumFallback::OtherString,
                    ComponentKind::Nutype(_) | ComponentKind::Range(_) => false,
                };
                if generic {
                    family.generic.insert(component.rust_name.clone());
                }
            }
            if previous == family.generic.len() {
                break;
            }
        }
        for operation in &api.operations {
            if input_fields(operation).iter().any(|f| family.uses(&f.ty)) {
                family.generic.insert(operation.input_name.clone());
            }
            if operation
                .responses
                .iter()
                .any(|r| r.body.as_ref().is_some_and(|ty| family.uses(ty)))
            {
                family.generic.insert(operation.response_name.clone());
            }
        }
        family
    }

    /// The storage type parameter, named to avoid generated type names.
    pub fn storage(&self) -> &syn::Ident {
        &self.parameter
    }

    /// Whether the named component, input, or response is family-generic.
    pub fn is_generic(&self, name: &str) -> bool {
        self.generic.contains(name)
    }

    pub fn input_is_generic(&self, operation: &Operation) -> bool {
        self.is_generic(&operation.input_name)
    }

    pub fn response_is_generic(&self, operation: &Operation) -> bool {
        self.is_generic(&operation.response_name)
    }

    /// Whether a type mentions the storage family anywhere.
    pub fn uses(&self, ty: &TypeRef) -> bool {
        match ty {
            TypeRef::String | TypeRef::Array(_) | TypeRef::Map(_) => true,
            TypeRef::Option(inner) => self.uses(inner),
            TypeRef::Named(name) => self.is_generic(name),
            _ => false,
        }
    }

    /// Whether a value is itself a family projection rather than a model that
    /// contains one. Projections implement no traits beyond the read contract,
    /// so they are formatted, compared, cloned, and serialized through a shape.
    fn projects(&self, ty: &TypeRef) -> bool {
        match ty {
            TypeRef::String | TypeRef::Array(_) | TypeRef::Map(_) => true,
            TypeRef::Option(inner) => self.projects(inner),
            TypeRef::Named(name) => self.projected_alias(name).is_some(),
            _ => false,
        }
    }

    /// The target of an alias that names a family projection.
    pub fn projected_alias(&self, name: &str) -> Option<&TypeRef> {
        self.aliases
            .get(name)
            .filter(|target| self.projects(target))
    }

    pub fn ty(&self, ty: &TypeRef, spelling: Spelling) -> syn::Type {
        let storage = &self.parameter;
        let family = match spelling {
            Spelling::Bounded => quote!(#storage),
            Spelling::Qualified => quote!(<#storage as satay_runtime::Storage>),
            Spelling::Concrete => quote!(),
        };
        let concrete = spelling == Spelling::Concrete;
        match ty {
            TypeRef::String if concrete => parse_quote!(String),
            TypeRef::String => parse_quote!(#family::Text<'s>),
            TypeRef::Array(item) => {
                let item = self.ty(item, spelling);
                if concrete {
                    parse_quote!(Vec<#item>)
                } else {
                    parse_quote!(#family::Contiguous<'s, #item>)
                }
            }
            TypeRef::Map(value) => {
                let value = self.ty(value, spelling);
                if concrete {
                    parse_quote!(BTreeMap<String, #value>)
                } else {
                    parse_quote!(#family::Map<'s, #value>)
                }
            }
            TypeRef::Option(inner) => {
                let inner = self.ty(inner, spelling);
                parse_quote!(Option<#inner>)
            }
            TypeRef::Named(name) if self.is_generic(name) => {
                let name = ident(name);
                if concrete {
                    parse_quote!(#name<'static>)
                } else {
                    parse_quote!(#name<'s, #storage>)
                }
            }
            other => plain_type(other),
        }
    }

    /// A struct field's type: optional and lossy fields are wrapped in `Option`.
    pub fn field_ty(&self, ty: &TypeRef, optional: bool, spelling: Spelling) -> syn::Type {
        let rendered = self.ty(ty, spelling);
        if optional && !ty.is_option() {
            parse_quote!(Option<#rendered>)
        } else {
            rendered
        }
    }

    /// The shape marker mirroring `ty`, with policy-independent leaves as `Plain`.
    pub fn shape(&self, ty: &TypeRef) -> TokenStream {
        match ty {
            TypeRef::String => quote!(shape::Text),
            TypeRef::Array(item) => {
                let item = self.shape(item);
                quote!(shape::Contiguous<#item>)
            }
            TypeRef::Map(value) => {
                let value = self.shape(value);
                quote!(shape::Map<#value>)
            }
            TypeRef::Option(inner) => {
                let inner = self.shape(inner);
                quote!(shape::Optional<#inner>)
            }
            TypeRef::Named(name) if self.projected_alias(name).is_some() => {
                self.shape(&self.aliases[name])
            }
            TypeRef::Named(name) if self.is_generic(name) => {
                let model = self.ty(ty, Spelling::Bounded);
                quote!(shape::Model<#model>)
            }
            other => {
                let plain = self.ty(other, Spelling::Bounded);
                quote!(shape::Plain<#plain>)
            }
        }
    }

    /// The shape of a struct field, including the `Option` added for optional fields.
    pub fn field_shape(&self, ty: &TypeRef, optional: bool) -> TokenStream {
        let shape = self.shape(ty);
        if optional && !ty.is_option() {
            quote!(shape::Optional<#shape>)
        } else {
            shape
        }
    }

    /// The shape through which a field is read, or `None` when its own trait
    /// implementations suffice.
    pub fn view(&self, ty: &TypeRef, optional: bool) -> Option<TokenStream> {
        self.projects(ty).then(|| self.field_shape(ty, optional))
    }

    /// The shape through which a field is decoded, or `None` when it implements
    /// `Deserialize` independently of the storage policy.
    pub fn seed(&self, ty: &TypeRef, optional: bool) -> Option<TokenStream> {
        self.uses(ty).then(|| self.field_shape(ty, optional))
    }

    /// Parameters declaring a family-generic item, with the default policy.
    pub fn declaration(&self) -> TokenStream {
        let storage = &self.parameter;
        quote!('s, #storage: satay_runtime::Storage + 's = satay_runtime::AllocStorage)
    }

    /// Parameters of an implementation needing only the read contract.
    pub fn parameters(&self) -> TokenStream {
        let storage = &self.parameter;
        quote!('s, #storage: satay_runtime::Storage + 's)
    }

    /// Parameters of an implementation that decodes into a storage context.
    pub fn decoding_parameters(&self) -> TokenStream {
        let storage = &self.parameter;
        quote!('s, #storage: satay_runtime::Storage<Error: fmt::Display> + 's)
    }

    /// The bound for constructing values without a context or failure.
    pub fn owned_bound(&self) -> TokenStream {
        quote!(satay_runtime::OwnedStorage<Error = convert::Infallible>)
    }

    /// The bound for decoding without a caller-provided context.
    pub fn owned_decoding_bound(&self) -> TokenStream {
        quote!(satay_runtime::OwnedStorage<Error: fmt::Display>)
    }

    /// Arguments naming a family-generic item inside a generic context.
    pub fn arguments(&self) -> TokenStream {
        let storage = &self.parameter;
        quote!('s, #storage)
    }

    /// `Debug`, `Clone`, and `PartialEq` for a family-generic struct.
    ///
    /// Derives cannot be used: they would bound the policy type itself and
    /// require traits the family's projections do not promise.
    pub fn struct_impls(&self, name: &syn::Ident, fields: &[Member]) -> Vec<syn::Item> {
        let storage = &self.parameter;
        let parameters = self.parameters();
        let arguments = self.arguments();
        let owned = self.owned_bound();
        let label = lit_str(&name.to_string());
        let debug = fields.iter().map(|field| {
            let name = &field.name;
            let label = lit_str(&name.unraw().to_string());
            let value = field.debug(storage, quote!(&self.#name));
            quote!(.field(#label, #value))
        });
        let clone = fields.iter().map(|field| {
            let name = &field.name;
            let value = match &field.view {
                Some(shape) => quote!(shape::duplicate::<#shape, #storage>(&self.#name)),
                // A path call: the field may be `Copy`, which a method call would
                // trip clippy on in generated code.
                None => quote!(Clone::clone(&self.#name)),
            };
            quote!(#name: #value)
        });
        let eq = fields.iter().map(|field| {
            let name = &field.name;
            match &field.view {
                Some(shape) => {
                    quote!(shape::eq::<#shape, #storage>(&self.#name, &other.#name))
                }
                None => quote!(self.#name == other.#name),
            }
        });
        let eq = if fields.is_empty() {
            quote!(true)
        } else {
            quote!(#(#eq)&&*)
        };
        let other = if fields.is_empty() {
            quote!(_)
        } else {
            quote!(other)
        };

        vec![
            parse_quote!(
                impl<#parameters> fmt::Debug for #name<#arguments> {
                    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.debug_struct(#label) #(#debug)* .finish()
                    }
                }
            ),
            parse_quote!(
                impl<'s, #storage: #owned> Clone for #name<#arguments> {
                    fn clone(&self) -> Self {
                        Self { #(#clone),* }
                    }
                }
            ),
            parse_quote!(
                impl<#parameters> PartialEq for #name<#arguments> {
                    fn eq(&self, #other: &Self) -> bool {
                        #eq
                    }
                }
            ),
        ]
    }

    /// `Debug`, `Clone`, and `PartialEq` for a family-generic enum of unit and
    /// tuple variants.
    pub fn enum_impls(&self, name: &syn::Ident, variants: &[Variant]) -> Vec<syn::Item> {
        let storage = &self.parameter;
        let parameters = self.parameters();
        let arguments = self.arguments();
        let owned = self.owned_bound();
        let debug = variants.iter().map(|variant| {
            let name = &variant.name;
            let label = lit_str(&name.unraw().to_string());
            let left = variant.bindings("left");
            if left.is_empty() {
                return quote!(Self::#name => f.write_str(#label));
            }
            let fields = variant
                .views
                .iter()
                .zip(&left)
                .map(|(view, binding)| match view {
                    Some(shape) => quote!(&shape::view::<#shape, #storage>(#binding)),
                    None => quote!(#binding),
                });
            quote!(Self::#name(#(#left),*) => f.debug_tuple(#label) #(.field(#fields))* .finish())
        });
        let clone = variants.iter().map(|variant| {
            let name = &variant.name;
            let left = variant.bindings("left");
            if left.is_empty() {
                return quote!(Self::#name => Self::#name);
            }
            let fields = variant
                .views
                .iter()
                .zip(&left)
                .map(|(view, binding)| match view {
                    Some(shape) => quote!(shape::duplicate::<#shape, #storage>(#binding)),
                    None => quote!(Clone::clone(#binding)),
                });
            quote!(Self::#name(#(#left),*) => Self::#name(#(#fields),*))
        });
        let eq =
            variants.iter().map(|variant| {
                let name = &variant.name;
                let (left, right) = (variant.bindings("left"), variant.bindings("right"));
                if left.is_empty() {
                    return quote!((Self::#name, Self::#name) => true);
                }
                let fields = variant.views.iter().zip(left.iter().zip(&right)).map(
                    |(view, (left, right))| match view {
                        Some(shape) => quote!(shape::eq::<#shape, #storage>(#left, #right)),
                        None => quote!(#left == #right),
                    },
                );
                quote!((Self::#name(#(#left),*), Self::#name(#(#right),*)) => #(#fields)&&*)
            });
        let mismatch = (variants.len() > 1).then(|| quote!(_ => false,));

        vec![
            parse_quote!(
                impl<#parameters> fmt::Debug for #name<#arguments> {
                    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        match self {
                            #(#debug,)*
                        }
                    }
                }
            ),
            parse_quote!(
                impl<'s, #storage: #owned> Clone for #name<#arguments> {
                    fn clone(&self) -> Self {
                        match self {
                            #(#clone,)*
                        }
                    }
                }
            ),
            parse_quote!(
                impl<#parameters> PartialEq for #name<#arguments> {
                    fn eq(&self, other: &Self) -> bool {
                        match (self, other) {
                            #(#eq,)*
                            #mismatch
                        }
                    }
                }
            ),
        ]
    }
}

/// A named struct field and the shape it is read through, if any.
pub(crate) struct Member {
    pub name: syn::Ident,
    pub view: Option<TokenStream>,
}

impl Member {
    fn debug(&self, storage: &syn::Ident, reference: TokenStream) -> TokenStream {
        match &self.view {
            Some(shape) => quote!(&shape::view::<#shape, #storage>(#reference)),
            None => reference,
        }
    }
}

/// An enum variant and the shapes its tuple fields are read through.
pub(crate) struct Variant {
    pub name: syn::Ident,
    pub views: Vec<Option<TokenStream>>,
}

impl Variant {
    fn bindings(&self, side: &str) -> Vec<syn::Ident> {
        (0..self.views.len())
            .map(|index| ident(&format!("{side}_{index}")))
            .collect()
    }
}

fn storage_parameter(api: &Api) -> syn::Ident {
    let reserved = api
        .components
        .iter()
        .map(|c| c.rust_name.as_str())
        .chain(api.constrained_types.iter().map(|c| c.rust_name.as_str()))
        .collect::<BTreeSet<_>>();
    let mut parameter = "S".to_owned();
    let mut suffix = 2;
    while reserved.contains(parameter.as_str()) {
        parameter = format!("S{suffix}");
        suffix += 1;
    }
    ident(&parameter)
}
