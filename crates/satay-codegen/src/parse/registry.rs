use std::collections::BTreeSet;

use crate::ident::{type_ident, unique_ident};
use crate::model::{
    Component, ComponentKind, ConstrainedType, EnumVariant, RangeScalar, RangeType, RangeTypeRef,
    TypeRef, Validation,
};

#[derive(Debug, Default)]
pub(crate) struct TypeRegistry {
    generated: Vec<ConstrainedType>,
    inline_enums: Vec<Component>,
    inline_ranges: Vec<Component>,
    used_names: BTreeSet<String>,
}

impl TypeRegistry {
    pub(crate) fn reserve(&mut self, rust_name: String) {
        self.used_names.insert(rust_name);
    }

    pub(crate) fn reserve_preferred_type_name(
        &mut self,
        candidates: impl IntoIterator<Item = String>,
    ) -> String {
        let mut last_candidate = None;

        for candidate in candidates {
            if !self.used_names.contains(&candidate) {
                self.used_names.insert(candidate.clone());
                return candidate;
            }
            last_candidate = Some(candidate);
        }

        unique_ident(
            last_candidate.expect("reserve_preferred_type_name requires at least one candidate"),
            &mut self.used_names,
        )
    }

    pub(crate) fn constrained_ref(
        &mut self,
        type_name_hint: &str,
        description: Option<String>,
        inner: TypeRef,
        validation: Validation,
    ) -> TypeRef {
        let rust_name = self.generated_type_name(type_name_hint);

        self.generated.push(ConstrainedType {
            rust_name: rust_name.clone(),
            description,
            inner: inner.clone(),
            validation,
        });

        TypeRef::Constrained {
            rust_name,
            inner: Box::new(inner),
        }
    }

    pub(crate) fn inline_enum_ref(
        &mut self,
        type_name_hint: &str,
        description: Option<String>,
        variants: Vec<EnumVariant>,
    ) -> TypeRef {
        let rust_name = self.generated_type_name(type_name_hint);

        self.inline_enums.push(Component {
            rust_name: rust_name.clone(),
            description,
            kind: ComponentKind::Enum(variants),
        });

        TypeRef::Named(rust_name)
    }

    pub(crate) fn inline_range_ref(
        &mut self,
        type_name_hint: &str,
        description: Option<String>,
        scalar: RangeScalar,
    ) -> TypeRef {
        let rust_name = self.generated_type_name(type_name_hint);

        self.inline_ranges.push(Component {
            rust_name: rust_name.clone(),
            description: description.clone(),
            kind: ComponentKind::Range(RangeType {
                rust_name: rust_name.clone(),
                description,
                scalar,
            }),
        });

        TypeRef::Range(RangeTypeRef { rust_name, scalar })
    }

    pub(crate) fn finish(
        self,
        mut components: Vec<Component>,
    ) -> (Vec<Component>, Vec<ConstrainedType>) {
        components.extend(self.inline_enums);
        components.extend(self.inline_ranges);
        (components, self.generated)
    }

    fn generated_type_name(&mut self, type_name_hint: &str) -> String {
        let candidate = type_ident(type_name_hint);
        if self.used_names.insert(candidate.clone()) {
            return candidate;
        }

        let candidate = format!("{candidate}_{}", stable_suffix(type_name_hint));
        unique_ident(candidate, &mut self.used_names)
    }
}

fn stable_suffix(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:08X}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::IntegerType;

    #[test]
    fn stable_suffix_is_deterministic() {
        assert_eq!(stable_suffix(""), "CBF29CE484222325");
        assert_eq!(stable_suffix("User name"), "B9537F8B37389455");
        assert_ne!(stable_suffix("User name"), stable_suffix("user name"));
    }

    #[test]
    fn honors_reserved_names_when_allocating_generated_types() {
        let mut registry = TypeRegistry::default();
        registry.reserve("UserName".to_owned());

        let ty = registry.constrained_ref(
            "User name",
            None,
            TypeRef::String,
            Validation::String {
                min_length: Some(1),
                max_length: None,
                pattern: None,
            },
        );

        match ty {
            TypeRef::Constrained { rust_name, inner } => {
                assert_eq!(
                    rust_name,
                    format!("UserName_{}", stable_suffix("User name"))
                );
                assert_eq!(inner.as_ref(), &TypeRef::String);
            }
            other => panic!("expected constrained ref, got {other:?}"),
        }
    }

    #[test]
    fn reserve_preferred_type_name_uses_first_available_candidate() {
        let mut registry = TypeRegistry::default();
        registry.reserve("PsiResponse".to_owned());

        let allocated = registry.reserve_preferred_type_name([
            "PsiResponse".to_owned(),
            "PsiOperationResponse".to_owned(),
        ]);
        let next = registry.reserve_preferred_type_name(["PsiOperationResponse".to_owned()]);

        assert_eq!(allocated, "PsiOperationResponse");
        assert_eq!(next, "PsiOperationResponse_2");
    }

    #[test]
    fn allocates_stable_collision_suffixes_and_numeric_tie_breakers() {
        let mut registry = TypeRegistry::default();
        registry.reserve("UserName".to_owned());
        registry.reserve(format!("UserName_{}", stable_suffix("User name")));

        let first = registry.inline_enum_ref(
            "User name",
            None,
            vec![EnumVariant {
                wire_name: "active".to_owned(),
                rust_name: "Active".to_owned(),
            }],
        );
        let second =
            registry.inline_range_ref("User name", None, RangeScalar::Integer(IntegerType::U8));

        assert_eq!(
            first,
            TypeRef::Named(format!("UserName_{}_2", stable_suffix("User name")))
        );
        assert_eq!(
            second,
            TypeRef::Range(RangeTypeRef {
                rust_name: format!("UserName_{}_3", stable_suffix("User name")),
                scalar: RangeScalar::Integer(IntegerType::U8),
            })
        );
    }

    #[test]
    fn accumulates_inline_enums_ranges_and_constrained_types_on_finish() {
        let mut registry = TypeRegistry::default();

        let constrained = registry.constrained_ref(
            "Search term",
            Some("Search text.".to_owned()),
            TypeRef::String,
            Validation::String {
                min_length: Some(2),
                max_length: Some(80),
                pattern: None,
            },
        );
        let inline_enum = registry.inline_enum_ref(
            "Search state",
            None,
            vec![EnumVariant {
                wire_name: "open".to_owned(),
                rust_name: "Open".to_owned(),
            }],
        );
        let inline_range = registry.inline_range_ref(
            "Search window",
            None,
            RangeScalar::Integer(IntegerType::U16),
        );

        assert!(matches!(constrained, TypeRef::Constrained { .. }));
        assert_eq!(inline_enum, TypeRef::Named("SearchState".to_owned()));
        assert_eq!(
            inline_range,
            TypeRef::Range(RangeTypeRef {
                rust_name: "SearchWindow".to_owned(),
                scalar: RangeScalar::Integer(IntegerType::U16),
            })
        );

        let base = vec![Component {
            rust_name: "Existing".to_owned(),
            description: None,
            kind: ComponentKind::Alias(TypeRef::String),
        }];
        let (components, constrained_types) = registry.finish(base);

        assert_eq!(
            components
                .iter()
                .map(|component| component.rust_name.as_str())
                .collect::<Vec<_>>(),
            ["Existing", "SearchState", "SearchWindow"]
        );
        assert_eq!(constrained_types.len(), 1);
        assert_eq!(constrained_types[0].rust_name, "SearchTerm");
    }
}
