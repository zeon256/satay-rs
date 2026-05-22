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
