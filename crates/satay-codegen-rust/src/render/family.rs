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
//!
//! A family-generic item is defined once, but named twice: the root module
//! aliases its name to the default policy, so code that never chooses a policy
//! sees an ordinary type, and [`Family::module`] re-exports the generic form.

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt;
use syn::parse_quote;

use super::{ident, input_fields, lit_str, plain_type};
use crate::model::{Api, ComponentKind, EnumFallback, Operation, TypeRef};

pub(crate) struct Family {
    parameter: syn::Ident,
    module: syn::Ident,
    generic: BTreeSet<String>,
    aliases: BTreeMap<String, TypeRef>,
}

/// How a shape treats the family-generic models inside it.
#[derive(Clone, Copy)]
enum Leaf {
    /// `shape::Plain`: the model's own trait implementations.
    Own,
    /// `shape::Model`: the model's decoder into a storage context.
    Context,
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
            module: generic_module(api),
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

    /// The root module re-exporting every family-generic definition. The root
    /// itself gives their names to the default policy.
    pub fn module(&self) -> &syn::Ident {
        &self.module
    }

    /// Whether the named component, input, or response is family-generic.
    pub fn is_generic(&self, name: &str) -> bool {
        self.generic.contains(name)
    }

    /// Splits names a child of the root module imports from it: those the root
    /// re-exports as they are, and an import of the family-generic ones, which
    /// the root aliases, from [`Self::module`].
    pub fn root_imports(&self, names: Vec<syn::Ident>) -> (Vec<syn::Ident>, Option<syn::ItemUse>) {
        let (generic, plain): (Vec<_>, Vec<_>) = names
            .into_iter()
            .partition(|name| self.is_generic(&name.to_string()));
        let module = &self.module;
        let generic_use =
            (!generic.is_empty()).then(|| parse_quote!(use super::#module::{#(#generic),*};));
        (plain, generic_use)
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

    /// The shape marker mirroring `ty`. Everything that is not a family
    /// projection is a `Plain` leaf using its own trait implementations.
    pub fn shape(&self, ty: &TypeRef) -> TokenStream {
        self.shape_with(ty, Leaf::Own)
    }

    /// The shape decoding `ty` into a storage context: family-generic models
    /// are `Model` leaves, decoded through their generated context decoder.
    pub fn seed_shape(&self, ty: &TypeRef) -> TokenStream {
        self.shape_with(ty, Leaf::Context)
    }

    fn shape_with(&self, ty: &TypeRef, leaf: Leaf) -> TokenStream {
        match ty {
            TypeRef::String => quote!(shape::Text),
            TypeRef::Array(item) => {
                let item = self.shape_with(item, leaf);
                quote!(shape::Contiguous<#item>)
            }
            TypeRef::Map(value) => {
                let value = self.shape_with(value, leaf);
                quote!(shape::Map<#value>)
            }
            TypeRef::Option(inner) => {
                let inner = self.shape_with(inner, leaf);
                quote!(shape::Optional<#inner>)
            }
            TypeRef::Named(name) if self.projected_alias(name).is_some() => {
                self.shape_with(&self.aliases[name], leaf)
            }
            TypeRef::Named(name) if self.is_generic(name) && matches!(leaf, Leaf::Context) => {
                let model = self.ty(ty, Spelling::Bounded);
                quote!(shape::Model<#model>)
            }
            other => {
                let plain = self.ty(other, Spelling::Bounded);
                quote!(shape::Plain<#plain>)
            }
        }
    }

    /// Wraps a field's shape in the `Option` added for optional fields.
    fn field_shape(shape: TokenStream, ty: &TypeRef, optional: bool) -> TokenStream {
        if optional && !ty.is_option() {
            quote!(shape::Optional<#shape>)
        } else {
            shape
        }
    }

    /// The shape through which a field is read and, for owned policies,
    /// decoded; `None` when its own trait implementations suffice.
    pub fn view(&self, ty: &TypeRef, optional: bool) -> Option<TokenStream> {
        self.projects(ty)
            .then(|| Self::field_shape(self.shape(ty), ty, optional))
    }

    /// The shape through which a field is decoded into a storage context, or
    /// `None` when it implements `Deserialize` independently of the policy.
    pub fn seed(&self, ty: &TypeRef, optional: bool) -> Option<TokenStream> {
        self.uses(ty)
            .then(|| Self::field_shape(self.seed_shape(ty), ty, optional))
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
    pub fn owned_bound() -> TokenStream {
        quote!(satay_runtime::OwnedStorage<Error = convert::Infallible>)
    }

    /// The bound for decoding without a caller-provided context.
    pub fn owned_decoding_bound() -> TokenStream {
        quote!(satay_runtime::OwnedStorage<Error: fmt::Display>)
    }

    /// Arguments naming a family-generic item inside a generic context.
    pub fn arguments(&self) -> TokenStream {
        let storage = &self.parameter;
        quote!('s, #storage)
    }

    /// Serde derives for a family-generic item. Serializing needs only the read
    /// contract; deserializing builds values through an owned policy.
    pub fn serde_derives(&self) -> TokenStream {
        let owned = lit_str(&format!(
            "{}: satay_runtime::OwnedStorage<Error: fmt::Display>",
            self.parameter
        ));
        quote!(
            #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
            #[cfg_attr(feature = "serde", serde(bound(serialize = "", deserialize = #owned)))]
        )
    }

    /// Serde attributes sending a projected value through `shape`: decoded for
    /// owned policies, serialized for any.
    pub fn shape_codec(&self, shape: &TokenStream) -> [TokenStream; 2] {
        let storage = &self.parameter;
        let deserialize = path(quote!(shape::owned::<#shape, #storage, _>));
        let serialize = path(quote!(shape::serialize::<#shape, #storage, _>));
        [
            quote!(deserialize_with = #deserialize),
            quote!(serialize_with = #serialize),
        ]
    }

    /// As [`Self::shape_codec`] for an optional value of shape `view`, whose
    /// `inner` shape decodes as `None` when the input does not match it.
    pub fn lossy_shape_codec(&self, view: &TokenStream, inner: &TokenStream) -> [TokenStream; 2] {
        let storage = &self.parameter;
        let deserialize = path(quote!(shape::lossy_owned::<#inner, #storage, _>));
        let serialize = path(quote!(shape::serialize::<#view, #storage, _>));
        [
            quote!(deserialize_with = #deserialize),
            quote!(serialize_with = #serialize),
        ]
    }

    /// `Debug`, `Clone`, and `PartialEq` for a family-generic struct.
    ///
    /// Derives cannot be used: they would bound the policy type itself and
    /// require traits the family's projections do not promise.
    pub fn struct_impls(&self, name: &syn::Ident, fields: &[Member]) -> Vec<syn::Item> {
        let storage = &self.parameter;
        let parameters = self.parameters();
        let arguments = self.arguments();
        let owned = Self::owned_bound();
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
        let owned = Self::owned_bound();
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

/// Names the root module of family-generic definitions, avoiding the other
/// root modules: operations, groups, and the fixed ones.
fn generic_module(api: &Api) -> syn::Ident {
    let reserved = api
        .operations
        .iter()
        .map(|operation| operation.fn_name.as_str())
        .chain(api.groups.iter().map(|group| group.rust_name.as_str()))
        .chain(["api", "types", "operations"])
        .collect::<BTreeSet<_>>();
    let mut module = "generic".to_owned();
    let mut suffix = 2;
    while reserved.contains(module.as_str()) {
        module = format!("generic_{suffix}");
        suffix += 1;
    }
    ident(&module)
}

/// Renders a path for a Serde attribute, without the spaces `TokenStream` inserts.
fn path(tokens: TokenStream) -> syn::LitStr {
    lit_str(&tokens.to_string().replace(' ', "").replace(',', ", "))
}
