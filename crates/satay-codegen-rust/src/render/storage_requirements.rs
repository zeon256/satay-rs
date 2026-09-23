//! Storage requirements computed from the Rust schema graph before rendering.
//!
//! Container requirements are independent of their elements: `[i64]` requires
//! contiguous storage just as `[Pet]` does. Map keys remain concrete in the
//! family API, so only their values propagate requirements.

use std::collections::BTreeMap;

use crate::model::{Api, ComponentKind, EnumFallback, TypeRef};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Requirements {
    pub text: bool,
    pub contiguous: bool,
}

impl Requirements {
    fn merge(self, other: Self) -> Self {
        Self {
            text: self.text || other.text,
            contiguous: self.contiguous || other.contiguous,
        }
    }
}

pub(super) struct StorageRequirements {
    pub models: BTreeMap<String, Requirements>,
    pub inputs: BTreeMap<String, Requirements>,
    pub responses: BTreeMap<String, Requirements>,
}

impl StorageRequirements {
    pub fn new(api: &Api) -> Self {
        let mut models = BTreeMap::new();
        loop {
            let mut changed = false;
            for component in &api.components {
                let requirements = match &component.kind {
                    ComponentKind::Struct(fields) => {
                        combine(fields.iter().map(|field| of_type(&field.ty, &models)))
                    }
                    ComponentKind::Alias(ty) => of_type(ty, &models),
                    ComponentKind::Union(union) => combine(
                        union
                            .variants
                            .iter()
                            .map(|variant| of_type(&variant.ty, &models)),
                    ),
                    ComponentKind::Enum(value) => Requirements {
                        text: value.fallback == EnumFallback::OtherString,
                        ..Requirements::default()
                    },
                    ComponentKind::Nutype(_) | ComponentKind::Range(_) => Requirements::default(),
                };
                if models.get(&component.rust_name) != Some(&requirements) {
                    models.insert(component.rust_name.clone(), requirements);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let inputs = api
            .operations
            .iter()
            .map(|operation| {
                (
                    operation.input_name.clone(),
                    combine(
                        super::input_fields(operation)
                            .iter()
                            .map(|field| of_type(&field.ty, &models)),
                    ),
                )
            })
            .collect();
        let responses = api
            .operations
            .iter()
            .map(|operation| {
                (
                    operation.response_name.clone(),
                    combine(
                        operation
                            .responses
                            .iter()
                            .filter_map(|response| response.body.as_ref())
                            .map(|ty| of_type(ty, &models)),
                    ),
                )
            })
            .collect();
        Self {
            models,
            inputs,
            responses,
        }
    }
}

fn combine(values: impl IntoIterator<Item = Requirements>) -> Requirements {
    values
        .into_iter()
        .fold(Requirements::default(), Requirements::merge)
}

fn of_type(ty: &TypeRef, models: &BTreeMap<String, Requirements>) -> Requirements {
    match ty {
        TypeRef::String => Requirements {
            text: true,
            ..Requirements::default()
        },
        TypeRef::Array(inner) => Requirements {
            contiguous: true,
            ..of_type(inner, models)
        },
        TypeRef::Map(inner) => of_type(inner, models),
        TypeRef::Option(inner) => of_type(inner, models),
        TypeRef::Named(name) => models.get(name).copied().unwrap_or_default(),
        TypeRef::Coordinates(codec) => models.get(codec.target()).copied().unwrap_or_default(),
        _ => Requirements::default(),
    }
}

#[cfg(test)]
mod tests;
