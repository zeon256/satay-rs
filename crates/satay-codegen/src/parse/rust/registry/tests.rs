use super::*;
use crate::model::{EnumFallback, EnumVariant, IntegerType};

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

    let allocated = registry
        .reserve_preferred_type_name(["PsiResponse".to_owned(), "PsiOperationResponse".to_owned()]);
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
        Enum {
            variants: vec![EnumVariant {
                wire_name: "active".to_owned(),
                rust_name: "Active".to_owned(),
            }],
            fallback: EnumFallback::None,
        },
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
        Enum {
            variants: vec![EnumVariant {
                wire_name: "open".to_owned(),
                rust_name: "Open".to_owned(),
            }],
            fallback: EnumFallback::None,
        },
    );
    let inline_range = registry.inline_range_ref(
        "Search window",
        None,
        RangeScalar::Integer(IntegerType::U16),
    );
    let inline_struct = registry.inline_struct_ref(
        "Search result",
        None,
        vec![Field {
            wire_name: "id".to_owned(),
            identifier_words: None,
            rust_name: "id".to_owned(),
            description: None,
            ty: TypeRef::String,
            required: true,
            treat_error_as_none: false,
            none_if: vec![],
        }],
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
    assert_eq!(inline_struct, TypeRef::Named("SearchResult".to_owned()));

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
        ["Existing", "SearchResult", "SearchState", "SearchWindow"]
    );
    assert_eq!(constrained_types.len(), 1);
    assert_eq!(constrained_types[0].rust_name, "SearchTerm");
}
