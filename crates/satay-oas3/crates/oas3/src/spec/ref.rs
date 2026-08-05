use std::{borrow::Cow, fmt, marker::PhantomData, str::FromStr};

use derive_more::derive::{Display, Error};
use log::trace;
use serde::{Deserialize, Serialize};

use super::{
    Callback, Example, Header, Link, Parameter, PathItem, RequestBody, Response, Schema,
    SecurityScheme, Spec,
};

/// Container for a type of OpenAPI object, or a reference to one.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ObjectOrReference<T> {
    /// Object reference.
    ///
    /// See <https://spec.openapis.org/oas/v3.1.1#reference-object>.
    Ref {
        /// Path, file reference, or URL pointing to object.
        #[serde(rename = "$ref")]
        ref_path: String,

        /// Summary override.
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,

        /// Description override.
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },

    /// Inline object.
    Object(T),
}

impl<T> ObjectOrReference<T>
where
    T: FromRef,
{
    /// Resolves the object (if needed) from the given `spec` and returns it.
    pub fn resolve(&self, spec: &Spec) -> Result<T, RefError> {
        match self {
            Self::Object(component) => Ok(component.clone()),
            Self::Ref { ref_path, .. } => T::from_ref(spec, ref_path),
        }
    }
}

/// Object reference error.
#[derive(Debug, Clone, PartialEq, Display, Error)]
pub enum RefError {
    /// Referenced object has unknown type.
    #[display("Invalid type: {}", _0)]
    UnknownType(#[error(not(source))] String),

    /// Referenced object was not of expected type.
    #[display("Mismatched type: cannot reference a {} as a {}", _0, _1)]
    MismatchedType(RefType, RefType),

    /// Reference path points outside the given spec file.
    #[display("Unresolvable path: {}", _0)]
    Unresolvable(#[error(not(source))] String), // TODO: use some kind of path structure
}

/// Component type of a reference.
#[derive(Debug, Clone, Copy, PartialEq, Display)]
pub enum RefType {
    /// Schema component type.
    Schema,

    /// Response component type.
    Response,

    /// Parameter component type.
    Parameter,

    /// Example component type.
    Example,

    /// Request body component type.
    RequestBody,

    /// Header component type.
    Header,

    /// Security scheme component type.
    SecurityScheme,

    /// Link component type.
    Link,

    /// Callback component type.
    Callback,
}

impl FromStr for RefType {
    type Err = RefError;

    fn from_str(typ: &str) -> Result<Self, Self::Err> {
        Ok(match typ {
            "schemas" => Self::Schema,
            "responses" => Self::Response,
            "parameters" => Self::Parameter,
            "examples" => Self::Example,
            "requestBodies" => Self::RequestBody,
            "headers" => Self::Header,
            "securitySchemes" => Self::SecurityScheme,
            "links" => Self::Link,
            "callbacks" => Self::Callback,
            typ => return Err(RefError::UnknownType(typ.to_owned())),
        })
    }
}

mod component_target_private {
    pub trait Sealed {}
}

/// An AST type that can be stored in an OpenAPI [`Components`](super::Components) section.
///
/// This trait associates each component type with its JSON wire-format section name. It is sealed
/// because the sections of an OpenAPI Components Object are fixed by the specification.
pub trait ComponentTarget: component_target_private::Sealed {
    /// JSON wire-format name of this target's Components Object section.
    const COMPONENT_SECTION: &'static str;
}

macro_rules! impl_component_target {
    ($($target:ty => $section:literal),+ $(,)?) => {
        $(
            impl component_target_private::Sealed for $target {}

            impl ComponentTarget for $target {
                const COMPONENT_SECTION: &'static str = $section;
            }
        )+
    };
}

impl_component_target!(
    Schema => "schemas",
    Response => "responses",
    Parameter => "parameters",
    Example => "examples",
    RequestBody => "requestBodies",
    Header => "headers",
    SecurityScheme => "securitySchemes",
    Link => "links",
    Callback => "callbacks",
    PathItem => "pathItems",
);

/// Error parsing a typed local component reference.
#[derive(Debug, Clone, PartialEq, Eq, Display, Error)]
#[non_exhaustive]
pub enum ComponentRefError {
    /// The reference is not local to the current document.
    #[display("reference `{}` is not local to the current document", _0)]
    NotLocal(#[error(not(source))] String),

    /// The local reference is not a Components Object pointer.
    #[display("reference `{}` is not a component pointer", _0)]
    NotComponent(#[error(not(source))] String),

    /// The component pointer does not contain a component name.
    #[display("component reference `{}` has no component name", _0)]
    MissingName(#[error(not(source))] String),

    /// The pointer continues beyond the component name.
    #[display("component reference `{}` points inside a component", _0)]
    NestedPointer(#[error(not(source))] String),

    /// The component section does not match the requested target type.
    ///
    /// Fields contain the original reference, expected section, and actual section respectively.
    #[display(
        "component reference `{}` targets section `{}`; expected `{}`",
        _0,
        _2,
        _1
    )]
    WrongSection(
        #[error(not(source))] String,
        #[error(not(source))] &'static str,
        #[error(not(source))] String,
    ),

    /// The component name contains a malformed RFC 6901 escape.
    ///
    /// Fields contain the original reference and byte offset of the malformed `~` respectively.
    #[display(
        "component reference `{}` has a malformed JSON Pointer escape at byte {}",
        _0,
        _1
    )]
    InvalidEscape(#[error(not(source))] String, #[error(not(source))] usize),
}

impl ComponentRefError {
    /// Returns the original reference text that failed to parse.
    pub fn reference(&self) -> &str {
        match self {
            Self::NotLocal(reference)
            | Self::NotComponent(reference)
            | Self::MissingName(reference)
            | Self::NestedPointer(reference)
            | Self::WrongSection(reference, ..)
            | Self::InvalidEscape(reference, _) => reference,
        }
    }
}

/// A validated local reference to a typed OpenAPI component.
///
/// This parser intentionally accepts only `#/components/{section}/{name}` references. The original
/// text is retained for diagnostics while [`name`](Self::name) returns the RFC 6901-decoded
/// component name. File and HTTP loading belong to a general resolver rather than this type.
///
/// # Examples
///
/// ```
/// use oas3::spec::{LocalComponentRef, Schema};
///
/// let reference =
///     LocalComponentRef::<Schema>::parse("#/components/schemas/User~1Profile").unwrap();
///
/// assert_eq!(reference.name(), "User/Profile");
/// assert_eq!(reference.as_str(), "#/components/schemas/User~1Profile");
/// ```
pub struct LocalComponentRef<'a, T> {
    original: &'a str,
    name: Cow<'a, str>,
    target: PhantomData<fn() -> T>,
}

impl<T> fmt::Debug for LocalComponentRef<'_, T>
where
    T: ComponentTarget,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalComponentRef")
            .field("original", &self.original)
            .field("section", &T::COMPONENT_SECTION)
            .field("name", &self.name)
            .finish()
    }
}

impl<T> Clone for LocalComponentRef<'_, T> {
    fn clone(&self) -> Self {
        Self {
            original: self.original,
            name: self.name.clone(),
            target: PhantomData,
        }
    }
}

impl<T> PartialEq for LocalComponentRef<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.original == other.original && self.name == other.name
    }
}

impl<T> Eq for LocalComponentRef<'_, T> {}

impl<'a, T> LocalComponentRef<'a, T>
where
    T: ComponentTarget,
{
    /// Parses and validates a local reference for component type `T`.
    pub fn parse(reference: &'a str) -> Result<Self, ComponentRefError> {
        let pointer = parse_local_component_pointer(reference)?;

        if pointer.section != T::COMPONENT_SECTION {
            return Err(ComponentRefError::WrongSection(
                reference.to_owned(),
                T::COMPONENT_SECTION,
                pointer.section.to_owned(),
            ));
        }

        Ok(Self {
            original: reference,
            name: decode_json_pointer_token(pointer.encoded_name, reference, pointer.name_offset)?,
            target: PhantomData,
        })
    }

    /// Returns the original, encoded `$ref` text.
    pub fn as_str(&self) -> &'a str {
        self.original
    }

    /// Returns the decoded component name.
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    /// Returns the JSON wire-format Components Object section for `T`.
    pub fn section(&self) -> &'static str {
        T::COMPONENT_SECTION
    }
}

impl<'a, T> TryFrom<&'a str> for LocalComponentRef<'a, T>
where
    T: ComponentTarget,
{
    type Error = ComponentRefError;

    fn try_from(reference: &'a str) -> Result<Self, Self::Error> {
        Self::parse(reference)
    }
}

impl<T> AsRef<str> for LocalComponentRef<'_, T> {
    fn as_ref(&self) -> &str {
        self.original
    }
}

impl<T> fmt::Display for LocalComponentRef<'_, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.original)
    }
}

struct LocalComponentPointer<'a> {
    section: &'a str,
    encoded_name: &'a str,
    name_offset: usize,
}

fn parse_local_component_pointer(
    reference: &str,
) -> Result<LocalComponentPointer<'_>, ComponentRefError> {
    const COMPONENTS_PREFIX: &str = "#/components/";

    if !reference.starts_with('#') {
        return Err(ComponentRefError::NotLocal(reference.to_owned()));
    }

    let Some(component_path) = reference.strip_prefix(COMPONENTS_PREFIX) else {
        return Err(ComponentRefError::NotComponent(reference.to_owned()));
    };
    let Some((section, encoded_name)) = component_path.split_once('/') else {
        return Err(ComponentRefError::MissingName(reference.to_owned()));
    };

    if encoded_name.is_empty() {
        return Err(ComponentRefError::MissingName(reference.to_owned()));
    }
    if encoded_name.contains('/') {
        return Err(ComponentRefError::NestedPointer(reference.to_owned()));
    }

    Ok(LocalComponentPointer {
        section,
        encoded_name,
        name_offset: COMPONENTS_PREFIX.len() + section.len() + 1,
    })
}

fn decode_json_pointer_token<'a>(
    token: &'a str,
    reference: &str,
    token_offset: usize,
) -> Result<Cow<'a, str>, ComponentRefError> {
    if !token.contains('~') {
        return Ok(Cow::Borrowed(token));
    }

    let mut decoded = String::with_capacity(token.len());
    let mut characters = token.char_indices();

    while let Some((offset, character)) = characters.next() {
        if character != '~' {
            decoded.push(character);
            continue;
        }

        match characters.next() {
            Some((_, '0')) => decoded.push('~'),
            Some((_, '1')) => decoded.push('/'),
            _ => {
                return Err(ComponentRefError::InvalidEscape(
                    reference.to_owned(),
                    token_offset + offset,
                ));
            }
        }
    }

    Ok(Cow::Owned(decoded))
}

/// Parsed reference path.
#[derive(Debug, Clone)]
pub struct Ref {
    /// Source file of the object being references.
    pub source: String,

    /// Type of object being referenced.
    pub kind: RefType,

    /// Name of object being referenced.
    pub name: String,
}

impl FromStr for Ref {
    type Err = RefError;

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        let pointer = parse_local_component_pointer(path)
            .map_err(|_| RefError::Unresolvable(path.to_owned()))?;
        let kind = pointer.section.parse()?;
        let name = decode_json_pointer_token(pointer.encoded_name, path, pointer.name_offset)
            .map_err(|_| RefError::Unresolvable(path.to_owned()))?
            .into_owned();

        trace!("creating Ref: {}/{name}", pointer.section);

        Ok(Self {
            source: String::new(),
            kind,
            name,
        })
    }
}

/// Find an object from a reference path (`$ref`).
///
/// Implemented for object types which can be shared via a spec's `components` object.
pub trait FromRef: Clone {
    /// Finds an object in `spec` using the given `path`.
    fn from_ref(spec: &Spec, path: &str) -> Result<Self, RefError>;
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use assert_matches::assert_matches;
    use serde_json::json;

    use super::*;

    #[test]
    fn ref_serialization_omits_empty_overrides() {
        // A plain reference should not emit `null` summary/description slots.
        let reference = ObjectOrReference::<()>::Ref {
            ref_path: "#/components/examples/RustMascot".to_owned(),
            summary: None,
            description: None,
        };

        let serialized = serde_json::to_value(reference).expect("serializing ref");

        assert_eq!(
            serialized,
            json!({
                "$ref": "#/components/examples/RustMascot",
            })
        );
    }

    #[test]
    fn ref_serialization_includes_present_overrides() {
        // Explicit overrides must still be preserved during serialization.
        let reference = ObjectOrReference::<()>::Ref {
            ref_path: "#/components/examples/RustMascot".to_owned(),
            summary: Some("Rust mascot override".to_owned()),
            description: Some("Let Ferris do the talking.".to_owned()),
        };

        let serialized = serde_json::to_value(reference).expect("serializing ref");

        assert_eq!(
            serialized,
            json!({
                "$ref": "#/components/examples/RustMascot",
                "summary": "Rust mascot override",
                "description": "Let Ferris do the talking.",
            })
        );
    }

    #[test]
    fn parses_typed_local_component_references() {
        let reference = LocalComponentRef::<Schema>::parse("#/components/schemas/User")
            .expect("valid schema reference");

        assert_eq!(reference.as_str(), "#/components/schemas/User");
        assert_eq!(reference.name(), "User");
        assert_eq!(reference.section(), "schemas");
        assert_eq!(reference.to_string(), reference.as_str());
        assert_matches!(reference.name, Cow::Borrowed("User"));

        let converted = LocalComponentRef::<Schema>::try_from("#/components/schemas/User")
            .expect("valid schema reference");
        assert_eq!(converted, reference);
    }

    #[test]
    fn associates_every_component_section_with_its_ast_type() {
        fn assert_section<T>(reference: &str, expected: &'static str)
        where
            T: ComponentTarget,
        {
            let reference =
                LocalComponentRef::<T>::parse(reference).expect("valid component reference");
            assert_eq!(reference.section(), expected);
        }

        assert_section::<Schema>("#/components/schemas/Test", "schemas");
        assert_section::<Response>("#/components/responses/Test", "responses");
        assert_section::<Parameter>("#/components/parameters/Test", "parameters");
        assert_section::<Example>("#/components/examples/Test", "examples");
        assert_section::<RequestBody>("#/components/requestBodies/Test", "requestBodies");
        assert_section::<Header>("#/components/headers/Test", "headers");
        assert_section::<SecurityScheme>("#/components/securitySchemes/Test", "securitySchemes");
        assert_section::<Link>("#/components/links/Test", "links");
        assert_section::<Callback>("#/components/callbacks/Test", "callbacks");
        assert_section::<PathItem>("#/components/pathItems/Test", "pathItems");
    }

    #[test]
    fn decodes_json_pointer_tokens_in_component_names() {
        for (encoded, decoded) in [
            ("User~0Code", "User~Code"),
            ("User~1Profile", "User/Profile"),
            ("a~1b~0c", "a/b~c"),
            ("~01", "~1"),
        ] {
            let reference = format!("#/components/schemas/{encoded}");
            let parsed = LocalComponentRef::<Schema>::parse(&reference)
                .expect("valid escaped component name");

            assert_eq!(parsed.name(), decoded);
            assert_matches!(parsed.name, Cow::Owned(_));
        }
    }

    #[test]
    fn rejects_references_to_the_wrong_component_section() {
        let reference = "#/components/responses/NotFound";
        let error =
            LocalComponentRef::<Schema>::parse(reference).expect_err("a response is not a schema");

        assert_eq!(error.reference(), reference);
        assert_matches!(
            error,
            ComponentRefError::WrongSection(original, "schemas", actual)
                if original == reference && actual == "responses"
        );
    }

    #[test]
    fn rejects_non_local_non_component_and_incomplete_references() {
        for reference in [
            "other.yaml#/components/schemas/User",
            "https://example.test/openapi.yaml#/components/schemas/User",
        ] {
            let error = LocalComponentRef::<Schema>::parse(reference)
                .expect_err("external reference must be rejected");
            assert_eq!(error.reference(), reference);
            assert_matches!(error, ComponentRefError::NotLocal(_));
        }

        for reference in [
            "#/paths/~1users",
            "#User",
            "#/components-other/schemas/User",
        ] {
            let error = LocalComponentRef::<Schema>::parse(reference)
                .expect_err("non-component pointer must be rejected");
            assert_eq!(error.reference(), reference);
            assert_matches!(error, ComponentRefError::NotComponent(_));
        }

        for reference in [
            "#/components/",
            "#/components/schemas",
            "#/components/schemas/",
        ] {
            let error = LocalComponentRef::<Schema>::parse(reference)
                .expect_err("component name is required");
            assert_eq!(error.reference(), reference);
            assert_matches!(error, ComponentRefError::MissingName(_));
        }
    }

    #[test]
    fn rejects_nested_component_pointers_and_malformed_escapes() {
        let nested = "#/components/schemas/User/properties/name";
        let error = LocalComponentRef::<Schema>::parse(nested)
            .expect_err("nested pointer must be rejected");
        assert_eq!(error.reference(), nested);
        assert_matches!(error, ComponentRefError::NestedPointer(_));

        for reference in [
            "#/components/schemas/User~2Name",
            "#/components/schemas/User~",
        ] {
            let error = LocalComponentRef::<Schema>::parse(reference)
                .expect_err("malformed escape must be rejected");
            let expected_offset = reference.find('~').expect("test reference has tilde");

            assert_eq!(error.reference(), reference);
            assert_matches!(
                error,
                ComponentRefError::InvalidEscape(original, offset)
                    if original == reference && offset == expected_offset
            );
        }
    }

    #[test]
    fn legacy_ref_parser_uses_validated_decoded_local_references() {
        let reference = "#/components/schemas/a~1b~0c"
            .parse::<Ref>()
            .expect("valid local component reference");

        assert_eq!(reference.source, "");
        assert_eq!(reference.kind, RefType::Schema);
        assert_eq!(reference.name, "a/b~c");

        let external = "other.yaml#/components/schemas/User"
            .parse::<Ref>()
            .expect_err("legacy lookup cannot resolve external documents");
        assert_matches!(
            external,
            RefError::Unresolvable(path)
                if path == "other.yaml#/components/schemas/User"
        );
    }

    #[test]
    fn invalid_ref_path_returns_error() {
        let err = "/components/schemas/petdetails#pet_details_id"
            .parse::<Ref>()
            .expect_err("invalid $ref should not parse");

        assert_matches!(
            err,
            RefError::Unresolvable(path) if path == "/components/schemas/petdetails#pet_details_id"
        );
    }
}
