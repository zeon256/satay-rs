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

    pub(crate) fn constrained_ref(
        &mut self,
        type_name_hint: &str,
        description: Option<String>,
        inner: TypeRef,
        validation: Validation,
    ) -> TypeRef {
        let rust_name = unique_ident(type_ident(type_name_hint), &mut self.used_names);

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
        let rust_name = unique_ident(type_ident(type_name_hint), &mut self.used_names);

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
        let rust_name = unique_ident(type_ident(type_name_hint), &mut self.used_names);

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
}
