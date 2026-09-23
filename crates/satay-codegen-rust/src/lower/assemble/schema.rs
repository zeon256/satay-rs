use crate::ident::{field_ident, type_ident, unique_ident, variant_ident};
use crate::lower::checked::{
    CheckedComponent, CheckedComponentKind, CheckedField, CheckedFieldValue, CheckedType,
    CheckedTypeKind, CheckedUnion, CheckedUnionTagStyle, CheckedUnionVariant,
    CheckedUnionVariantKind,
};
use crate::lower::registry::TypeRegistry;
use crate::model::{
    Component, ComponentKind, ConstrainedType, CoordinateCodec, Enum, Field, RangeType,
    RangeTypeRef, TypeRef, Union, UnionTag, UnionTagStyle, UnionVariant,
};

use std::collections::{BTreeMap, BTreeSet};

pub(super) struct SchemaLowerer<'a> {
    components: &'a [CheckedComponent],
    component_kinds: BTreeMap<String, ComponentKind>,
    component_refs: BTreeMap<String, TypeRef>,
}

impl<'a> SchemaLowerer<'a> {
    pub(super) fn new(components: &'a [CheckedComponent]) -> Self {
        Self {
            components,
            component_kinds: BTreeMap::new(),
            component_refs: BTreeMap::new(),
        }
    }

    pub(super) fn parse_components(&mut self, registry: &mut TypeRegistry) -> Vec<Component> {
        self.components
            .iter()
            .map(|component| self.parse_component(component, registry))
            .collect()
    }

    pub(super) fn parse_type_ref_with_hint(
        &mut self,
        ty: &CheckedType,
        type_name_hint: &str,
        registry: &mut TypeRegistry,
    ) -> TypeRef {
        let mut parsed =
            self.parse_type_ref_base(&ty.kind, type_name_hint, &ty.description, registry);

        if let Some(validation) = &ty.validation {
            parsed = registry.constrained_ref(
                type_name_hint,
                ty.description.clone(),
                parsed,
                validation.clone(),
            );
        }

        if ty.nullable {
            TypeRef::option(parsed)
        } else {
            parsed
        }
    }

    fn parse_component(
        &mut self,
        component: &CheckedComponent,
        registry: &mut TypeRegistry,
    ) -> Component {
        let rust_name = type_ident(&component.schema_name);
        let kind = self.parse_component_kind(component, registry);

        Component {
            rust_name,
            description: component.description.clone(),
            kind,
        }
    }

    fn parse_component_kind(
        &mut self,
        component: &CheckedComponent,
        registry: &mut TypeRegistry,
    ) -> ComponentKind {
        let rust_name = type_ident(&component.schema_name);
        if let Some(kind) = self.component_kinds.get(&rust_name) {
            return kind.clone();
        }

        let kind = match &component.kind {
            CheckedComponentKind::Reference(reference) => {
                ComponentKind::Alias(self.component_ref(reference, registry))
            }
            CheckedComponentKind::Struct(fields) => ComponentKind::Struct(
                self.parse_struct_fields(&component.schema_name, fields, registry),
            ),
            CheckedComponentKind::Type(ty) => self.parse_component_type(
                &component.schema_name,
                &component.description,
                ty,
                registry,
            ),
        };

        self.component_kinds.insert(rust_name, kind.clone());
        kind
    }

    fn parse_component_type(
        &mut self,
        schema_name: &str,
        description: &Option<String>,
        ty: &CheckedType,
        registry: &mut TypeRegistry,
    ) -> ComponentKind {
        let rust_name = type_ident(schema_name);

        match (&ty.kind, ty.nullable, ty.validation.as_ref()) {
            (CheckedTypeKind::Enum(enum_), false, None) => ComponentKind::Enum(enum_.clone()),
            (CheckedTypeKind::AnyOf(union), false, None) => {
                ComponentKind::Union(self.parse_union(union, schema_name, registry))
            }
            (CheckedTypeKind::Range(scalar), false, None) => ComponentKind::Range(RangeType {
                rust_name,
                description: description.clone(),
                scalar: *scalar,
            }),
            (_, false, Some(validation)) => ComponentKind::Nutype(ConstrainedType {
                rust_name,
                description: description.clone(),
                inner: self.parse_type_ref_base(&ty.kind, schema_name, &ty.description, registry),
                validation: validation.clone(),
            }),
            (_, true, Some(validation)) => {
                let hint = format!("{schema_name} value");
                let base =
                    self.parse_type_ref_base(&ty.kind, schema_name, &ty.description, registry);
                let inner = registry.constrained_ref(
                    &hint,
                    ty.description.clone(),
                    base,
                    validation.clone(),
                );
                ComponentKind::Alias(TypeRef::option(inner))
            }
            (CheckedTypeKind::Enum(_) | CheckedTypeKind::Range(_), true, None) => {
                let hint = format!("{schema_name} value");
                ComponentKind::Alias(self.parse_type_ref_with_hint(ty, &hint, registry))
            }
            _ => ComponentKind::Alias(self.parse_type_ref_with_hint(ty, schema_name, registry)),
        }
    }

    fn parse_struct_fields(
        &mut self,
        schema_name: &str,
        fields: &[CheckedField],
        registry: &mut TypeRegistry,
    ) -> Vec<Field> {
        let mut used = BTreeSet::new();
        let mut parsed = Vec::with_capacity(fields.len());

        for field in fields {
            let validated_ty = field.value.ty();
            let identifier = field
                .identifier
                .as_ref()
                .map(|identifier| identifier.join("-"))
                .unwrap_or_else(|| field.wire_name.clone());
            let ty = self.parse_type_ref_with_hint(
                validated_ty,
                &format!("{schema_name} {}", field.wire_name),
                registry,
            );
            let (treat_error_as_none, none_if) = match &field.value {
                CheckedFieldValue::Strict(_) => (false, vec![]),
                CheckedFieldValue::Lossy(_) => (true, vec![]),
                CheckedFieldValue::SentinelParsedString { sentinels, .. } => {
                    (false, sentinels.as_slice().to_vec())
                }
            };
            parsed.push(Field {
                wire_name: field.wire_name.clone(),
                identifier_words: field.identifier.clone(),
                rust_name: unique_ident(field_ident(&identifier), &mut used),
                description: field.description.clone(),
                ty,
                required: field.required,
                treat_error_as_none,
                none_if,
            });
        }

        parsed
    }

    fn parse_type_ref_base(
        &mut self,
        kind: &CheckedTypeKind,
        type_name_hint: &str,
        description: &Option<String>,
        registry: &mut TypeRegistry,
    ) -> TypeRef {
        match kind {
            CheckedTypeKind::Named(rust_name) => self.component_ref(rust_name, registry),
            CheckedTypeKind::String => TypeRef::String,
            CheckedTypeKind::ParsedString(codec) => TypeRef::ParsedString(codec.clone()),
            CheckedTypeKind::Coordinates(coordinates) => {
                let component = self.validated_component(coordinates.target());
                let target = self.parse_component_kind(&component, registry);
                TypeRef::Coordinates(CoordinateCodec::from_parts(
                    coordinates.target().to_owned(),
                    coordinates.field_indices(),
                    coordinates.delimiter().clone(),
                    &target,
                ))
            }
            CheckedTypeKind::ParsedInteger(parse_as) => TypeRef::ParsedInteger(*parse_as),
            CheckedTypeKind::Integer(integer_type) => TypeRef::Integer(*integer_type),
            CheckedTypeKind::F32 => TypeRef::F32,
            CheckedTypeKind::F64 => TypeRef::F64,
            CheckedTypeKind::Bool => TypeRef::Bool,
            CheckedTypeKind::Array(item) => TypeRef::Array(Box::new(
                self.parse_type_ref_with_hint(item, &format!("{type_name_hint} item"), registry),
            )),
            CheckedTypeKind::Map(value) => TypeRef::Map(Box::new(self.parse_type_ref_with_hint(
                value,
                &format!("{type_name_hint} value"),
                registry,
            ))),
            CheckedTypeKind::JsonValue => TypeRef::JsonValue,
            CheckedTypeKind::Enum(variants) => parse_inline_enum_ref(
                variants.clone(),
                type_name_hint,
                description.clone(),
                registry,
            ),
            CheckedTypeKind::AnyOf(union) => {
                // An untagged union with a single component reference accepts exactly
                // that component's payloads, so the wrapper enum adds nothing.
                if let Some(type_name) = single_reference_union_target(union) {
                    self.component_ref(type_name, registry)
                } else {
                    let union = self.parse_union(union, type_name_hint, registry);
                    registry.inline_union_ref(type_name_hint, description.clone(), union)
                }
            }
            CheckedTypeKind::InlineStruct(fields) => {
                let fields = self.parse_struct_fields(type_name_hint, fields, registry);
                registry.inline_struct_ref(type_name_hint, description.clone(), fields)
            }
            CheckedTypeKind::Range(scalar) => {
                registry.inline_range_ref(type_name_hint, description.clone(), *scalar)
            }
        }
    }

    fn parse_union(
        &mut self,
        union: &CheckedUnion,
        type_name_hint: &str,
        registry: &mut TypeRegistry,
    ) -> Union {
        Union {
            variants: self.parse_union_variants(&union.variants, type_name_hint, registry),
            tag: union.tag.as_ref().map(|tag| UnionTag {
                property_name: tag.property_name.clone(),
                style: match tag.style {
                    CheckedUnionTagStyle::InternallyTagged => UnionTagStyle::InternallyTagged,
                    CheckedUnionTagStyle::EmbeddedField => UnionTagStyle::EmbeddedField,
                },
            }),
        }
    }

    fn parse_union_variants(
        &mut self,
        variants: &[CheckedUnionVariant],
        type_name_hint: &str,
        registry: &mut TypeRegistry,
    ) -> Vec<UnionVariant> {
        variants
            .iter()
            .map(|variant| UnionVariant {
                rust_name: variant.rust_name.clone(),
                ty: match &variant.kind {
                    CheckedUnionVariantKind::Reference { type_name, .. } => {
                        self.component_ref(type_name, registry)
                    }
                    CheckedUnionVariantKind::Inline(ty) => self.parse_type_ref_with_hint(
                        ty,
                        &format!("{type_name_hint} {}", variant.rust_name),
                        registry,
                    ),
                },
                tag_value: variant.tag_value.clone(),
            })
            .collect()
    }

    fn component_ref(&mut self, rust_name: &str, registry: &mut TypeRegistry) -> TypeRef {
        if let Some(ty) = self.component_refs.get(rust_name) {
            return ty.clone();
        }

        let component = self.validated_component(rust_name);
        // Struct identity is known before its fields are lowered. Register it
        // first so self/mutual references do not recursively lower the fields.
        if matches!(component.kind, CheckedComponentKind::Struct(_)) {
            let ty = TypeRef::Named(rust_name.to_owned());
            self.component_refs.insert(rust_name.to_owned(), ty.clone());
            return ty;
        }
        let kind = self.parse_component_kind(&component, registry);
        let ty = match &kind {
            ComponentKind::Struct(_) | ComponentKind::Enum(_) | ComponentKind::Union(_) => {
                TypeRef::Named(rust_name.to_owned())
            }
            ComponentKind::Range(range_type) => TypeRef::Range(RangeTypeRef {
                rust_name: range_type.rust_name.clone(),
                scalar: range_type.scalar,
            }),
            ComponentKind::Alias(alias) => alias.clone(),
            ComponentKind::Nutype(constrained_type) => TypeRef::Constrained {
                rust_name: constrained_type.rust_name.clone(),
                inner: Box::new(constrained_type.inner.clone()),
            },
        };

        self.component_refs.insert(rust_name.to_owned(), ty.clone());
        ty
    }

    fn validated_component(&self, rust_name: &str) -> CheckedComponent {
        self.components
            .iter()
            .find(|component| type_ident(&component.schema_name) == rust_name)
            .cloned()
            .unwrap_or_else(|| panic!("component reference {rust_name} validated before lowering"))
    }
}

fn single_reference_union_target(union: &CheckedUnion) -> Option<&str> {
    if union.tag.is_some() || union.variants.len() != 1 {
        return None;
    }

    match &union.variants[0].kind {
        CheckedUnionVariantKind::Reference { type_name, .. } => Some(type_name),
        CheckedUnionVariantKind::Inline(_) => None,
    }
}

fn parse_inline_enum_ref(
    mut enum_: Enum,
    type_name_hint: &str,
    description: Option<String>,
    registry: &mut TypeRegistry,
) -> TypeRef {
    let default_empty_variant = variant_ident("");
    enum_.variants.retain(|variant| {
        !variant.wire_name.is_empty() || variant.rust_name != default_empty_variant
    });

    if enum_.variants.is_empty() {
        TypeRef::String
    } else {
        registry.inline_enum_ref(type_name_hint, description, enum_)
    }
}
