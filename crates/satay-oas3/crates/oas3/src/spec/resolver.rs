use std::{error::Error, fmt};

use super::{
    Callback, ComponentRefError, ComponentTarget, Components, Example, Header, Link,
    LocalComponentRef, ObjectOrReference, Parameter, PathItem, RequestBody, Response, Schema,
    SecurityScheme, Spec,
};

/// A component type stored as an [`ObjectOrReference`] in the Components Object.
///
/// This trait is sealed through [`ComponentTarget`] because OpenAPI component sections are fixed by
/// the specification.
pub trait ResolvableComponent: ComponentTarget + Sized {
    #[doc(hidden)]
    fn component<'a>(components: &'a Components, name: &str)
        -> Option<&'a ObjectOrReference<Self>>;
}

macro_rules! impl_resolvable_component {
    ($($target:ty => $field:ident),+ $(,)?) => {
        $(
            impl ResolvableComponent for $target {
                fn component<'a>(
                    components: &'a Components,
                    name: &str,
                ) -> Option<&'a ObjectOrReference<Self>> {
                    components.$field.get(name)
                }
            }
        )+
    };
}

impl_resolvable_component!(
    Response => responses,
    Parameter => parameters,
    Example => examples,
    RequestBody => request_bodies,
    Header => headers,
    PathItem => path_items,
    SecurityScheme => security_schemes,
    Link => links,
    Callback => callbacks,
);

/// Error navigating a local component reference.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// A `$ref` is not a valid local reference for the requested component type.
    InvalidReference(ComponentRefError),

    /// A valid reference targets a component that is not present.
    MissingComponent {
        /// Original `$ref` text.
        reference: String,

        /// JSON wire-format Components Object section.
        section: &'static str,

        /// Decoded component name.
        name: String,
    },

    /// Following references revisited a component already in the current chain.
    Cycle {
        /// Original `$ref` values from the start of the cycle through the repeated target.
        references: Vec<String>,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReference(source) => source.fmt(formatter),
            Self::MissingComponent {
                reference,
                section,
                name,
            } => write!(
                formatter,
                "component reference `{reference}` targets missing `{section}` component `{name}`"
            ),
            Self::Cycle { references } => write!(
                formatter,
                "component reference cycle detected: {}",
                references.join(" -> ")
            ),
        }
    }
}

impl Error for ResolveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidReference(source) => Some(source),
            Self::MissingComponent { .. } | Self::Cycle { .. } => None,
        }
    }
}

impl From<ComponentRefError> for ResolveError {
    fn from(source: ComponentRefError) -> Self {
        Self::InvalidReference(source)
    }
}

/// Navigates local component references in one already-loaded OpenAPI document.
///
/// Resolution returns references to the original AST and never clones or merges component values.
/// In particular, Schema Object `$ref` siblings and Reference Object overrides remain on the input
/// node rather than being applied to the returned target.
#[derive(Debug, Clone, Copy)]
pub struct Resolver<'spec> {
    spec: &'spec Spec,
}

impl<'spec> Resolver<'spec> {
    /// Creates a resolver for local references in `spec`.
    pub const fn new(spec: &'spec Spec) -> Self {
        Self { spec }
    }

    /// Resolves an inline or referenced reusable component without cloning it.
    ///
    /// Chained Reference Objects are followed until an inline component is reached.
    pub fn resolve<'a, T>(
        &'a self,
        component: &'a ObjectOrReference<T>,
    ) -> Result<&'a T, ResolveError>
    where
        T: ResolvableComponent,
    {
        let mut trace = ResolutionTrace { entries: vec![] };
        self.resolve_component(component, &mut trace)
    }

    /// Resolves a Schema Object `$ref` without cloning or merging sibling keywords.
    ///
    /// Boolean and inline object schemas are returned unchanged. Chained `$ref` values are followed
    /// until either kind of inline schema is reached.
    pub fn resolve_schema<'a>(&'a self, schema: &'a Schema) -> Result<&'a Schema, ResolveError> {
        let mut current = schema;
        let mut trace = ResolutionTrace { entries: vec![] };

        loop {
            let Some(reference) = current.reference() else {
                return Ok(current);
            };
            let reference = LocalComponentRef::<Schema>::parse(reference)?;
            trace.visit(&reference)?;
            current = self.schema(&reference)?;
        }
    }

    /// Resolves a Path Item Object `$ref` without cloning or merging sibling fields.
    ///
    /// Both direct Path Item `$ref` fields and Reference Objects stored in
    /// `components.pathItems` are followed as one chain.
    pub fn resolve_path_item<'a>(
        &'a self,
        path_item: &'a PathItem,
    ) -> Result<&'a PathItem, ResolveError> {
        let mut current = PathItemNode::Object(path_item);
        let mut trace = ResolutionTrace { entries: vec![] };

        loop {
            let raw_reference = match current {
                PathItemNode::Object(path_item) => match path_item.reference.as_deref() {
                    Some(reference) => reference,
                    None => return Ok(path_item),
                },
                PathItemNode::Component(ObjectOrReference::Object(path_item)) => {
                    current = PathItemNode::Object(path_item);
                    continue;
                }
                PathItemNode::Component(ObjectOrReference::Ref { ref_path, .. }) => ref_path,
            };

            let reference = LocalComponentRef::<PathItem>::parse(raw_reference)?;
            trace.visit(&reference)?;
            current = PathItemNode::Component(self.component(&reference)?);
        }
    }

    fn resolve_component<'a, T>(
        &'a self,
        mut component: &'a ObjectOrReference<T>,
        trace: &mut ResolutionTrace,
    ) -> Result<&'a T, ResolveError>
    where
        T: ResolvableComponent,
    {
        loop {
            match component {
                ObjectOrReference::Object(component) => return Ok(component),
                ObjectOrReference::Ref { ref_path, .. } => {
                    let reference = LocalComponentRef::<T>::parse(ref_path)?;
                    trace.visit(&reference)?;
                    component = self.component(&reference)?;
                }
            }
        }
    }

    fn component<'a, T>(
        &'a self,
        reference: &LocalComponentRef<'_, T>,
    ) -> Result<&'a ObjectOrReference<T>, ResolveError>
    where
        T: ResolvableComponent,
    {
        self.spec
            .components
            .as_ref()
            .and_then(|components| T::component(components, reference.name()))
            .ok_or_else(|| missing_component(reference))
    }

    fn schema<'a>(
        &'a self,
        reference: &LocalComponentRef<'_, Schema>,
    ) -> Result<&'a Schema, ResolveError> {
        self.spec
            .components
            .as_ref()
            .and_then(|components| components.schemas.get(reference.name()))
            .ok_or_else(|| missing_component(reference))
    }
}

fn missing_component<T>(reference: &LocalComponentRef<'_, T>) -> ResolveError
where
    T: ComponentTarget,
{
    ResolveError::MissingComponent {
        reference: reference.as_str().to_owned(),
        section: reference.section(),
        name: reference.name().to_owned(),
    }
}

enum PathItemNode<'a> {
    Object(&'a PathItem),
    Component(&'a ObjectOrReference<PathItem>),
}

struct ResolutionTrace {
    entries: Vec<ResolutionTraceEntry>,
}

struct ResolutionTraceEntry {
    section: &'static str,
    name: String,
    reference: String,
}

impl ResolutionTrace {
    fn visit<T>(&mut self, reference: &LocalComponentRef<'_, T>) -> Result<(), ResolveError>
    where
        T: ComponentTarget,
    {
        if let Some(cycle_start) = self.entries.iter().position(|entry| {
            entry.section == reference.section() && entry.name == reference.name()
        }) {
            let mut references = self.entries[cycle_start..]
                .iter()
                .map(|entry| entry.reference.clone())
                .collect::<Vec<_>>();
            references.push(reference.as_str().to_owned());
            return Err(ResolveError::Cycle { references });
        }

        self.entries.push(ResolutionTraceEntry {
            section: reference.section(),
            name: reference.name().to_owned(),
            reference: reference.as_str().to_owned(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use serde_json::json;

    use super::*;
    use crate::spec::{ObjectSchema, SchemaType as Type, SchemaTypeSet as TypeSet};

    fn empty_spec() -> Spec {
        serde_json::from_value(json!({
            "openapi": "3.1.0",
            "info": {
                "title": "Resolver tests",
                "version": "1.0.0",
            },
        }))
        .expect("valid minimal spec")
    }

    fn reference<T>(path: &str) -> ObjectOrReference<T> {
        ObjectOrReference::Ref {
            ref_path: path.to_owned(),
            summary: None,
            description: None,
        }
    }

    fn parameter(name: &str) -> Parameter {
        serde_json::from_value(json!({
            "name": name,
            "in": "query",
            "schema": { "type": "string" },
        }))
        .expect("valid parameter")
    }

    fn schema_reference(path: &str, description: Option<&str>) -> Schema {
        Schema::Object(Box::new(ObjectSchema {
            reference: Some(path.to_owned()),
            description: description.map(str::to_owned),
            ..ObjectSchema::default()
        }))
    }

    #[test]
    fn every_object_or_reference_component_type_is_resolvable() {
        fn assert_resolvable<T: ResolvableComponent>() {}

        assert_resolvable::<Response>();
        assert_resolvable::<Parameter>();
        assert_resolvable::<Example>();
        assert_resolvable::<RequestBody>();
        assert_resolvable::<Header>();
        assert_resolvable::<PathItem>();
        assert_resolvable::<SecurityScheme>();
        assert_resolvable::<Link>();
        assert_resolvable::<Callback>();
    }

    #[test]
    fn resolves_inline_and_chained_components_by_borrowing() {
        let mut inline = ObjectOrReference::Object(parameter("inline"));
        let spec = empty_spec();
        let resolver = Resolver::new(&spec);
        let expected_inline = match &inline {
            ObjectOrReference::Object(parameter) => parameter,
            ObjectOrReference::Ref { .. } => unreachable!(),
        };
        assert!(std::ptr::eq(
            resolver.resolve(&inline).expect("inline parameter"),
            expected_inline
        ));

        let mut spec = empty_spec();
        let mut components = Components::default();
        components.parameters.insert(
            "PublicId".to_owned(),
            reference("#/components/parameters/InternalId"),
        );
        components.parameters.insert(
            "InternalId".to_owned(),
            reference("#/components/parameters/Actual~1Id"),
        );
        components.parameters.insert(
            "Actual/Id".to_owned(),
            ObjectOrReference::Object(parameter("id")),
        );
        spec.components = Some(components);

        let resolver = Resolver::new(&spec);
        let components = spec.components.as_ref().expect("components");
        let public = &components.parameters["PublicId"];
        let actual = match &components.parameters["Actual/Id"] {
            ObjectOrReference::Object(parameter) => parameter,
            ObjectOrReference::Ref { .. } => unreachable!(),
        };
        let resolved = resolver.resolve(public).expect("chained parameter");

        assert!(std::ptr::eq(resolved, actual));
        assert_eq!(resolved.name, "id");

        inline = reference("#/components/parameters/PublicId");
        assert!(std::ptr::eq(
            resolver.resolve(&inline).expect("external input reference"),
            actual
        ));
    }

    #[test]
    fn reports_invalid_and_missing_component_references() {
        let spec = empty_spec();
        let resolver = Resolver::new(&spec);

        let wrong_section = reference::<Parameter>("#/components/responses/NotFound");
        let error = resolver
            .resolve(&wrong_section)
            .expect_err("wrong section must fail");
        assert_matches!(
            error,
            ResolveError::InvalidReference(ComponentRefError::WrongSection(
                reference,
                "parameters",
                actual,
            )) if reference == "#/components/responses/NotFound" && actual == "responses"
        );

        let missing = reference::<Parameter>("#/components/parameters/Missing~1Id");
        let error = resolver
            .resolve(&missing)
            .expect_err("missing target must fail");
        assert_matches!(
            error,
            ResolveError::MissingComponent {
                reference,
                section: "parameters",
                name,
            } if reference == "#/components/parameters/Missing~1Id" && name == "Missing/Id"
        );
    }

    #[test]
    fn detects_component_reference_cycles_with_the_original_chain() {
        let mut spec = empty_spec();
        let mut components = Components::default();
        components
            .parameters
            .insert("A".to_owned(), reference("#/components/parameters/B"));
        components
            .parameters
            .insert("B".to_owned(), reference("#/components/parameters/A"));
        spec.components = Some(components);

        let root = reference::<Parameter>("#/components/parameters/A");
        let error = Resolver::new(&spec)
            .resolve(&root)
            .expect_err("cycle must fail");

        assert_matches!(
            error,
            ResolveError::Cycle { references } if references == [
                "#/components/parameters/A",
                "#/components/parameters/B",
                "#/components/parameters/A",
            ]
        );
    }

    #[test]
    fn resolves_schema_chains_without_merging_siblings() {
        let mut spec = empty_spec();
        let mut components = Components::default();
        components.schemas.insert(
            "Alias".to_owned(),
            schema_reference("#/components/schemas/Target", None),
        );
        components.schemas.insert(
            "Target".to_owned(),
            Schema::Object(Box::new(ObjectSchema {
                schema_type: Some(TypeSet::Single(Type::String)),
                description: Some("target description".to_owned()),
                ..ObjectSchema::default()
            })),
        );
        spec.components = Some(components);

        let root = schema_reference(
            "#/components/schemas/Alias",
            Some("reference sibling description"),
        );
        let resolver = Resolver::new(&spec);
        let resolved = resolver.resolve_schema(&root).expect("schema chain");
        let expected = &spec.components.as_ref().expect("components").schemas["Target"];

        assert!(std::ptr::eq(resolved, expected));
        assert_eq!(resolved.description(), Some("target description"));
        assert_eq!(root.description(), Some("reference sibling description"));

        let boolean = Schema::Boolean(super::super::BooleanSchema(true));
        assert!(std::ptr::eq(
            resolver.resolve_schema(&boolean).expect("boolean schema"),
            &boolean
        ));
    }

    #[test]
    fn detects_schema_reference_cycles() {
        let mut spec = empty_spec();
        let mut components = Components::default();
        components.schemas.insert(
            "A".to_owned(),
            schema_reference("#/components/schemas/B", None),
        );
        components.schemas.insert(
            "B".to_owned(),
            schema_reference("#/components/schemas/A", None),
        );
        spec.components = Some(components);

        let root = schema_reference("#/components/schemas/A", None);
        let error = Resolver::new(&spec)
            .resolve_schema(&root)
            .expect_err("schema cycle must fail");

        assert_matches!(
            error,
            ResolveError::Cycle { references } if references == [
                "#/components/schemas/A",
                "#/components/schemas/B",
                "#/components/schemas/A",
            ]
        );
    }

    #[test]
    fn resolves_path_item_reference_objects_and_direct_refs_as_one_chain() {
        let mut spec = empty_spec();
        let mut components = Components::default();
        components
            .path_items
            .insert("A".to_owned(), reference("#/components/pathItems/B"));
        components.path_items.insert(
            "B".to_owned(),
            ObjectOrReference::Object(PathItem {
                reference: Some("#/components/pathItems/C".to_owned()),
                ..PathItem::default()
            }),
        );
        components.path_items.insert(
            "C".to_owned(),
            ObjectOrReference::Object(PathItem {
                summary: Some("resolved path item".to_owned()),
                ..PathItem::default()
            }),
        );
        spec.components = Some(components);

        let root = PathItem {
            reference: Some("#/components/pathItems/A".to_owned()),
            summary: Some("reference sibling".to_owned()),
            ..PathItem::default()
        };
        let resolver = Resolver::new(&spec);
        let resolved = resolver.resolve_path_item(&root).expect("path item chain");
        let expected = match &spec.components.as_ref().expect("components").path_items["C"] {
            ObjectOrReference::Object(path_item) => path_item,
            ObjectOrReference::Ref { .. } => unreachable!(),
        };

        assert!(std::ptr::eq(resolved, expected));
        assert_eq!(resolved.summary.as_deref(), Some("resolved path item"));
        assert_eq!(root.summary.as_deref(), Some("reference sibling"));
    }

    #[test]
    fn detects_cycles_across_both_path_item_reference_shapes() {
        let mut spec = empty_spec();
        let mut components = Components::default();
        components.path_items.insert(
            "A".to_owned(),
            ObjectOrReference::Object(PathItem {
                reference: Some("#/components/pathItems/B".to_owned()),
                ..PathItem::default()
            }),
        );
        components
            .path_items
            .insert("B".to_owned(), reference("#/components/pathItems/A"));
        spec.components = Some(components);

        let root = PathItem {
            reference: Some("#/components/pathItems/A".to_owned()),
            ..PathItem::default()
        };
        let error = Resolver::new(&spec)
            .resolve_path_item(&root)
            .expect_err("path item cycle must fail");

        assert_matches!(
            error,
            ResolveError::Cycle { references } if references == [
                "#/components/pathItems/A",
                "#/components/pathItems/B",
                "#/components/pathItems/A",
            ]
        );
    }
}
