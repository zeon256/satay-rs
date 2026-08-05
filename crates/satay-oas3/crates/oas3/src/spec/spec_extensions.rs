use std::{error::Error, fmt};

use serde::{de, Deserialize, Deserializer, Serializer};

use super::{
    Callback, Components, Contact, Example, ExternalDoc, Flows, Header, ImplicitFlow, Info,
    License, MediaType, ObjectSchema, Operation, Parameter, PathItem, Response, Schema, Server,
    ServerVariable, Spec, Tag,
};
use crate::Map;

/// An error accessing a typed specification extension.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExtensionError {
    /// The requested name is not an OpenAPI specification-extension name.
    InvalidName {
        /// The rejected extension name.
        name: String,
    },

    /// The extension value could not be deserialized into the requested type.
    InvalidValue {
        /// The requested extension name.
        extension: String,

        /// The path to the invalid value, including the extension name.
        path: String,

        /// The underlying deserialization error.
        source: serde_json::Error,
    },
}

impl fmt::Display for ExtensionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName { name } => write!(
                formatter,
                "invalid specification extension name `{name}`; expected an `x-` prefix"
            ),
            Self::InvalidValue { path, source, .. } => write!(
                formatter,
                "failed to deserialize specification extension at `{path}`: {source}"
            ),
        }
    }
}

impl Error for ExtensionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidName { .. } => None,
            Self::InvalidValue { source, .. } => Some(source),
        }
    }
}

/// Typed access to the specification extensions stored on an OpenAPI object.
pub trait SpecificationExtensions {
    /// Deserializes one named specification extension into a consumer-owned type.
    ///
    /// `name` uses its wire-format `x-` prefix. A missing extension returns `Ok(None)`. Other
    /// extensions are not inspected.
    fn extension_as<'de, T>(&'de self, name: &str) -> Result<Option<T>, ExtensionError>
    where
        T: Deserialize<'de>;
}

pub(super) fn deserialize_extension<'de, T>(
    extensions: &'de Map<String, serde_json::Value>,
    name: &str,
) -> Result<Option<T>, ExtensionError>
where
    T: Deserialize<'de>,
{
    let key = extension_key(name)?;
    let Some(value) = extensions.get(key) else {
        return Ok(None);
    };

    serde_path_to_error::deserialize(value)
        .map(Some)
        .map_err(|error| {
            let nested_path = error.path().to_string();
            let path = if nested_path == "." {
                name.to_owned()
            } else if nested_path.starts_with('[') {
                format!("{name}{nested_path}")
            } else {
                format!("{name}.{nested_path}")
            };

            ExtensionError::InvalidValue {
                extension: name.to_owned(),
                path,
                source: error.into_inner(),
            }
        })
}

fn extension_key(name: &str) -> Result<&str, ExtensionError> {
    name.strip_prefix("x-")
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ExtensionError::InvalidName {
            name: name.to_owned(),
        })
}

macro_rules! impl_specification_extensions {
    ($($type:ty),+ $(,)?) => {
        $(
            impl SpecificationExtensions for $type {
                fn extension_as<'de, T>(
                    &'de self,
                    name: &str,
                ) -> Result<Option<T>, ExtensionError>
                where
                    T: Deserialize<'de>,
                {
                    deserialize_extension(&self.extensions, name)
                }
            }
        )+
    };
}

impl_specification_extensions!(
    Callback,
    Components,
    Contact,
    Example,
    ExternalDoc,
    Flows,
    Header,
    ImplicitFlow,
    Info,
    License,
    MediaType,
    ObjectSchema,
    Operation,
    Parameter,
    PathItem,
    Response,
    Server,
    ServerVariable,
    Spec,
    Tag,
);

impl SpecificationExtensions for Schema {
    fn extension_as<'de, T>(&'de self, name: &str) -> Result<Option<T>, ExtensionError>
    where
        T: Deserialize<'de>,
    {
        match self {
            Self::Boolean(_) => {
                extension_key(name)?;
                Ok(None)
            }
            Self::Object(schema) => deserialize_extension(&schema.extensions, name),
        }
    }
}

/// Deserializes fields of a map beginning with `x-`.
pub(crate) fn deserialize<'de, D>(
    deserializer: D,
) -> Result<Map<String, serde_json::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ExtraFieldsVisitor;

    impl<'de> de::Visitor<'de> for ExtraFieldsVisitor {
        type Value = Map<String, serde_json::Value>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("extensions")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let mut map = Map::<String, serde_json::Value>::new();

            while let Some((key, value)) = access.next_entry()? {
                map.insert(key, value);
            }

            Ok(map
                .into_iter()
                .filter_map(|(key, value)| {
                    key.strip_prefix("x-").map(|key| (key.to_owned(), value))
                })
                .collect())
        }
    }

    deserializer.deserialize_map(ExtraFieldsVisitor)
}

/// Serializes fields of a map prefixed with `x-`.
pub(crate) fn serialize<S>(
    extensions: &Map<String, serde_json::Value>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.collect_map(
        extensions
            .iter()
            .map(|(key, value)| (format!("x-{key}"), value)),
    )
}

#[cfg(test)]
mod typed_tests {
    use serde::Deserialize;
    use serde_json::json;

    use super::*;
    use crate::spec::Link;

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "kebab-case", deny_unknown_fields)]
    struct BorrowedOptions<'a> {
        parse_as: &'a str,
        none_if: Vec<&'a str>,
    }

    #[test]
    fn deserializes_only_the_named_extension_into_borrowed_values() {
        let mut schema = ObjectSchema::default();
        schema.extensions.insert(
            "satay".to_owned(),
            json!({
                "parse-as": "date",
                "none-if": ["", "unknown"],
            }),
        );
        schema
            .extensions
            .insert("vendor-docs".to_owned(), json!({ "hidden": true }));

        let options: BorrowedOptions<'_> = schema
            .extension_as("x-satay")
            .expect("valid extension")
            .expect("present extension");

        assert_eq!(
            options,
            BorrowedOptions {
                parse_as: "date",
                none_if: vec!["", "unknown"],
            }
        );
        assert!(schema.extensions.contains_key("vendor-docs"));

        let missing: Option<BorrowedOptions<'_>> = schema
            .extension_as("x-missing")
            .expect("missing extensions are valid");
        assert_eq!(missing, None);
    }

    #[test]
    fn reports_the_extension_and_nested_invalid_value_path() {
        let mut operation = Operation::default();
        operation.extensions.insert(
            "satay".to_owned(),
            json!({ "parse-as": "date", "none-if": ["", 42] }),
        );

        let error = operation
            .extension_as::<BorrowedOptions<'_>>("x-satay")
            .expect_err("number is not a string");

        let ExtensionError::InvalidValue {
            extension,
            path,
            source,
        } = error
        else {
            panic!("expected an invalid extension value");
        };

        assert_eq!(extension, "x-satay");
        assert_eq!(path, "x-satay.none-if[1]");
        assert!(source.to_string().contains("invalid type: integer"));
    }

    #[test]
    fn consumer_types_control_unknown_fields_inside_the_named_extension() {
        let mut schema = ObjectSchema::default();
        schema.extensions.insert(
            "satay".to_owned(),
            json!({
                "parse-as": "date",
                "none-if": [],
                "unexpected": true,
            }),
        );

        let error = schema
            .extension_as::<BorrowedOptions<'_>>("x-satay")
            .expect_err("deny_unknown_fields should reject unexpected");

        let ExtensionError::InvalidValue {
            extension, source, ..
        } = error
        else {
            panic!("expected an invalid extension value");
        };

        assert_eq!(extension, "x-satay");
        assert!(source.to_string().contains("unknown field `unexpected`"));
    }

    #[test]
    fn rejects_names_without_a_wire_format_extension_prefix() {
        let schema = ObjectSchema::default();
        let error = schema
            .extension_as::<BorrowedOptions<'_>>("satay")
            .expect_err("extension name should require x- prefix");

        assert!(matches!(
            error,
            ExtensionError::InvalidName { name } if name == "satay"
        ));
    }

    #[test]
    fn callback_extensions_use_the_same_stripped_storage_and_typed_access() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Options {
            enabled: bool,
        }

        let callback = serde_json::from_value::<Callback>(json!({
            "x-satay": { "enabled": true },
        }))
        .expect("callback");

        assert!(callback.extensions.contains_key("satay"));
        assert!(!callback.extensions.contains_key("x-satay"));
        assert_eq!(
            callback
                .extension_as::<Options>("x-satay")
                .expect("valid extension"),
            Some(Options { enabled: true })
        );

        let serialized = serde_json::to_value(callback).expect("serialized callback");
        assert_eq!(serialized["x-satay"]["enabled"], true);
    }

    #[test]
    fn every_extension_bearing_ast_type_implements_typed_access() {
        fn assert_implementation<T: SpecificationExtensions>() {}

        assert_implementation::<Callback>();
        assert_implementation::<Components>();
        assert_implementation::<Contact>();
        assert_implementation::<Example>();
        assert_implementation::<ExternalDoc>();
        assert_implementation::<Flows>();
        assert_implementation::<Header>();
        assert_implementation::<ImplicitFlow>();
        assert_implementation::<Info>();
        assert_implementation::<License>();
        assert_implementation::<Link>();
        assert_implementation::<MediaType>();
        assert_implementation::<ObjectSchema>();
        assert_implementation::<Operation>();
        assert_implementation::<Parameter>();
        assert_implementation::<PathItem>();
        assert_implementation::<Response>();
        assert_implementation::<Schema>();
        assert_implementation::<Server>();
        assert_implementation::<ServerVariable>();
        assert_implementation::<Spec>();
        assert_implementation::<Tag>();
    }
}

#[cfg(all(test, feature = "yaml-spec"))]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::Spec;

    #[test]
    fn spec_extensions_deserialize() {
        let spec = indoc::indoc! {"
            openapi: '3.1.0'
            info:
              title: test
              version: v1
            components: {}
            x-bar: true
            qux: true
        "};

        let spec = serde_saphyr::from_str::<Spec>(spec).unwrap();
        assert!(spec.components.is_some());
        assert!(!spec.extensions.contains_key("x-bar"));
        assert!(!spec.extensions.contains_key("qux"));
        assert_eq!(spec.extensions.get("bar").unwrap(), true);
    }

    #[test]
    fn spec_extensions_deserialize_with_numeric_yaml_key_nearby() {
        let spec = indoc::indoc! {"
            openapi: '3.1.0'
            info:
              title: test
              version: v1
            components: {}
            42: test numeric key doesn't break it
            x-bar: true
            44: test numeric key doesn't break it
        "};

        let spec = serde_saphyr::from_str::<Spec>(spec).unwrap();
        assert!(spec.components.is_some());
        assert!(!spec.extensions.contains_key("x-bar"));
        assert_eq!(spec.extensions.get("bar").unwrap(), true);
    }

    #[test]
    fn spec_extensions_serialize() {
        let spec = indoc::indoc! {"
            openapi: 3.1.0
            info:
              title: test
              version: v1
            components: {}
            x-bar: true
        "};

        let parsed_spec = serde_saphyr::from_str::<Spec>(spec).unwrap();
        let round_trip_spec = serde_saphyr::to_string(&parsed_spec).unwrap();

        assert_eq!(spec, round_trip_spec);
    }
}
