use super::*;
use crate::model::{Component, Field, IntegerType};

fn alias(name: &str, ty: TypeRef) -> Component {
    Component {
        rust_name: name.into(),
        description: None,
        kind: ComponentKind::Alias(ty),
    }
}

fn field(name: &str, ty: TypeRef) -> Field {
    Field {
        wire_name: name.into(),
        identifier_words: None,
        rust_name: name.into(),
        description: None,
        ty,
        required: true,
        treat_error_as_none: false,
        none_if: vec![],
    }
}

#[test]
fn requirements_propagate_through_recursive_models_aliases_and_map_values() {
    let api = Api::new(
        String::new(),
        vec![],
        vec![
            alias(
                "Outer",
                TypeRef::Option(Box::new(TypeRef::Map(Box::new(TypeRef::Named(
                    "Node".into(),
                ))))),
            ),
            Component {
                rust_name: "Node".into(),
                description: None,
                kind: ComponentKind::Struct(vec![
                    field(
                        "children",
                        TypeRef::Array(Box::new(TypeRef::Named("Node".into()))),
                    ),
                    field("counts", TypeRef::Named("Counts".into())),
                    field("name", TypeRef::String),
                ]),
            },
            alias(
                "Counts",
                TypeRef::Array(Box::new(TypeRef::Integer(IntegerType::I64))),
            ),
            alias(
                "Matrix",
                TypeRef::Array(Box::new(TypeRef::Named("Counts".into()))),
            ),
            alias("ConcreteMap", TypeRef::Map(Box::new(TypeRef::Bool))),
        ],
        vec![],
        vec![],
        vec![],
    );
    let requirements = StorageRequirements::new(&api);
    for name in ["Node", "Outer"] {
        assert!(requirements.models[name].text);
        assert!(requirements.models[name].contiguous);
    }
    for name in ["Counts", "Matrix"] {
        assert!(!requirements.models[name].text);
        assert!(requirements.models[name].contiguous);
    }
    let map = requirements.models["ConcreteMap"];
    assert!(!map.text && !map.contiguous);
}

#[test]
fn scalar_arrays_require_storage_even_when_elements_do_not() {
    for element in [
        TypeRef::Bool,
        TypeRef::Integer(IntegerType::I64),
        TypeRef::Named("ClosedEnum".into()),
        TypeRef::Constrained {
            rust_name: "NonEmpty".into(),
            inner: Box::new(TypeRef::String),
        },
    ] {
        let scalar = of_type(&element, &BTreeMap::new());
        assert!(!scalar.text && !scalar.contiguous);
        let array = of_type(&TypeRef::Array(Box::new(element)), &BTreeMap::new());
        assert!(!array.text && array.contiguous);
    }
}
