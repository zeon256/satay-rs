use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::error::{ParseError, ValidationError};
use crate::ident::{
    field_ident, function_ident, response_variant_ident, type_ident, unique_ident, variant_ident,
};
use crate::model::{
    Api, ApiKeyLocation, ApiKeySecurityScheme, Component, ComponentKind, ConstrainedType,
    EnumVariant, Field, FloatLimit, HttpMethod, IntegerLimit, Operation, Parameter,
    ParameterLocation, ParseAs, PathSegment, RequestBody, ResponseCase, TypeRef, Validation,
    is_array_type,
};

pub(crate) fn parse_api(document: &Value) -> Result<Api, ValidationError> {
    tracing::debug!("parsing API from document");
    let root = object(document, "OpenAPI document")?;
    let openapi = required_str(root, "openapi", "OpenAPI document")?;
    if !openapi.starts_with("3.0") {
        return Err(ValidationError::UnsupportedOpenApiVersion {
            version: openapi.to_owned(),
        });
    }

    let mut registry = TypeRegistry::default();
    let server_url = parse_server_url(document)?;
    let api_key_security_schemes = parse_api_key_security_schemes(document)?;
    reserve_component_type_names(document, &mut registry)?;
    let mut components = parse_components(document, &mut registry)?;
    let operations = parse_operations(document, &mut registry)?;
    components.extend(registry.inline_enums);

    Ok(Api {
        server_url,
        api_key_security_schemes,
        components,
        constrained_types: registry.generated,
        operations,
    })
}

#[derive(Debug, Default)]
struct TypeRegistry {
    generated: Vec<ConstrainedType>,
    inline_enums: Vec<Component>,
    used_names: BTreeSet<String>,
}

impl TypeRegistry {
    fn reserve(&mut self, rust_name: String) {
        self.used_names.insert(rust_name);
    }

    fn constrained_ref(
        &mut self,
        type_name_hint: &str,
        inner: TypeRef,
        validation: Validation,
    ) -> TypeRef {
        let rust_name = unique_ident(type_ident(type_name_hint), &mut self.used_names);
        self.generated.push(ConstrainedType {
            rust_name: rust_name.clone(),
            inner: inner.clone(),
            validation,
        });
        TypeRef::Constrained {
            rust_name,
            inner: Box::new(inner),
        }
    }

    fn inline_enum_ref(&mut self, type_name_hint: &str, variants: Vec<EnumVariant>) -> TypeRef {
        let rust_name = unique_ident(type_ident(type_name_hint), &mut self.used_names);
        self.inline_enums.push(Component {
            rust_name: rust_name.clone(),
            kind: ComponentKind::Enum(variants),
        });
        TypeRef::Named(rust_name)
    }
}

pub(crate) fn parse_document(spec: &str) -> Result<Value, ParseError> {
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(spec).map_err(ParseError::Document)?;
    serde_json::to_value(yaml).map_err(ParseError::NormalizeDocument)
}

fn parse_server_url(document: &Value) -> Result<String, ValidationError> {
    let root = object(document, "OpenAPI document")?;
    let Some(servers) = root.get("servers") else {
        return Ok(String::new());
    };
    let servers = servers
        .as_array()
        .ok_or_else(|| ValidationError::ExpectedArray {
            context: "OpenAPI document servers".to_owned(),
        })?;
    let Some(first) = servers.first() else {
        return Ok(String::new());
    };
    let context = "OpenAPI document servers[0]";
    let server = object(first, context)?;
    Ok(required_str(server, "url", context)?.to_owned())
}

fn parse_api_key_security_schemes(
    document: &Value,
) -> Result<Vec<ApiKeySecurityScheme>, ValidationError> {
    let root = object(document, "OpenAPI document")?;
    let Some(components) = optional_object(root, "components", "OpenAPI document")? else {
        return Ok(vec![]);
    };
    let Some(security_schemes) = optional_object(components, "securitySchemes", "components")?
    else {
        return Ok(vec![]);
    };

    let mut used = BTreeSet::from([
        "apply".to_owned(),
        "base_url".to_owned(),
        "http".to_owned(),
        "new".to_owned(),
    ]);
    let mut schemes = Vec::new();
    for (scheme_name, scheme) in security_schemes {
        let context = format!("security scheme `{scheme_name}`");
        let scheme = resolve_reference(document, scheme, &context)?;
        let scheme = object(scheme, &context)?;
        if scheme.get("type").and_then(Value::as_str) != Some("apiKey") {
            continue;
        }

        let location = match required_str(scheme, "in", &context)? {
            "header" => ApiKeyLocation::Header,
            "query" => ApiKeyLocation::Query,
            _ => continue,
        };
        let wire_name = required_str(scheme, "name", &context)?.to_owned();
        let rust_name = unique_ident(field_ident(&wire_name), &mut used);
        schemes.push(ApiKeySecurityScheme {
            location,
            wire_name,
            rust_name,
        });
    }

    Ok(schemes)
}

fn reserve_component_type_names(
    document: &Value,
    registry: &mut TypeRegistry,
) -> Result<(), ValidationError> {
    let root = object(document, "OpenAPI document")?;
    let Some(components) = optional_object(root, "components", "OpenAPI document")? else {
        return Ok(());
    };
    let Some(schemas) = optional_object(components, "schemas", "components")? else {
        return Ok(());
    };

    for schema_name in schemas.keys() {
        registry.reserve(type_ident(schema_name));
    }
    Ok(())
}

fn parse_components(
    document: &Value,
    registry: &mut TypeRegistry,
) -> Result<Vec<Component>, ValidationError> {
    let root = object(document, "OpenAPI document")?;
    let Some(components) = optional_object(root, "components", "OpenAPI document")? else {
        return Ok(vec![]);
    };
    let Some(schemas) = optional_object(components, "schemas", "components")? else {
        return Ok(vec![]);
    };

    let mut components = Vec::with_capacity(schemas.len());
    for (schema_name, schema) in schemas {
        let rust_name = type_ident(schema_name);
        let kind = parse_component_kind(schema_name, schema, registry)?;
        components.push(Component { rust_name, kind });
    }

    Ok(components)
}

fn parse_component_kind(
    schema_name: &str,
    schema: &Value,
    registry: &mut TypeRegistry,
) -> Result<ComponentKind, ValidationError> {
    if let Some(reference) = schema_ref(schema) {
        return Ok(ComponentKind::Alias(TypeRef::Named(schema_ref_type_name(
            reference,
        )?)));
    }

    let context = format!("schema `{schema_name}`");
    let schema = object(schema, &context)?;
    reject_composition(schema, &context)?;

    if schema.contains_key("enum") {
        return Ok(ComponentKind::Enum(parse_string_enum(schema, &context)?));
    }

    match schema.get("type").and_then(Value::as_str) {
        Some("object") | None if schema.contains_key("properties") => Ok(ComponentKind::Struct(
            parse_struct_fields(schema_name, schema, registry)?,
        )),
        Some("array") | Some("string") | Some("integer") | Some("number") | Some("boolean") => {
            parse_component_alias_or_nutype(schema_name, schema, registry)
        }
        Some(kind) => Err(ValidationError::UnsupportedComponentType {
            schema: schema_name.to_owned(),
            kind: kind.to_owned(),
        }),
        None => Err(ValidationError::MissingComponentSchemaType {
            schema: schema_name.to_owned(),
        }),
    }
}

fn parse_component_alias_or_nutype(
    schema_name: &str,
    schema: &Map<String, Value>,
    registry: &mut TypeRegistry,
) -> Result<ComponentKind, ValidationError> {
    let context = format!("schema `{schema_name}`");
    let rust_name = type_ident(schema_name);
    let nullable = schema
        .get("nullable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let base = parse_type_ref_base(schema, &context, registry, Some(schema_name))?;
    let validation = parse_validation(schema, &base, &context)?;

    match (validation, nullable) {
        (Some(validation), false) => Ok(ComponentKind::Nutype(ConstrainedType {
            rust_name,
            inner: base,
            validation,
        })),
        (Some(validation), true) => {
            let inner = registry.constrained_ref(&format!("{schema_name} value"), base, validation);
            Ok(ComponentKind::Alias(TypeRef::Nullable(Box::new(inner))))
        }
        (None, false) => Ok(ComponentKind::Alias(base)),
        (None, true) => Ok(ComponentKind::Alias(TypeRef::Nullable(Box::new(base)))),
    }
}

fn parse_string_enum(
    schema: &Map<String, Value>,
    context: &str,
) -> Result<Vec<EnumVariant>, ValidationError> {
    if let Some(kind) = schema.get("type").and_then(Value::as_str)
        && kind != "string"
    {
        return Err(ValidationError::UnsupportedEnumType {
            context: context.to_owned(),
            kind: kind.to_owned(),
        });
    }

    let values = schema
        .get("enum")
        .and_then(Value::as_array)
        .ok_or_else(|| ValidationError::NonArrayEnum {
            context: context.to_owned(),
        })?;
    if values.is_empty() {
        return Err(ValidationError::EmptyEnum {
            context: context.to_owned(),
        });
    }

    let mut wire_values = BTreeSet::new();
    let mut wire_names = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value.as_str() else {
            return Err(ValidationError::NonStringEnumValue {
                context: context.to_owned(),
            });
        };
        wire_values.insert(value.to_owned());
        wire_names.push(value);
    }

    let explicit_variants = parse_satay_enum_variants(schema, context, &wire_values)?;
    let mut used = BTreeSet::from(["Unknown".to_owned()]);
    for rust_name in explicit_variants.values() {
        if rust_name != "Unknown" {
            used.insert(rust_name.clone());
        }
    }

    let mut variants = Vec::with_capacity(values.len());
    for wire_name in wire_names {
        let rust_name = if let Some(rust_name) = explicit_variants.get(wire_name) {
            if rust_name == "Unknown" {
                continue;
            }
            rust_name.clone()
        } else {
            unique_ident(variant_ident(wire_name), &mut used)
        };
        variants.push(EnumVariant {
            wire_name: wire_name.to_owned(),
            rust_name,
        });
    }

    Ok(variants)
}

fn parse_satay_enum_variants(
    schema: &Map<String, Value>,
    context: &str,
    enum_values: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>, ValidationError> {
    let Some(satay) = optional_object(schema, "x-satay", context)? else {
        return Ok(BTreeMap::new());
    };
    let Some(value) = satay.get("enum-variants") else {
        return Ok(BTreeMap::new());
    };
    let Some(value) = value.as_object() else {
        return Err(ValidationError::InvalidSatayEnumVariants {
            context: context.to_owned(),
        });
    };

    let mut mappings = BTreeMap::new();
    let mut explicit_names = BTreeSet::new();
    for (wire_name, rust_name) in value {
        if !enum_values.contains(wire_name) {
            return Err(ValidationError::UnknownSatayEnumVariantValue {
                context: context.to_owned(),
                wire_name: wire_name.clone(),
            });
        }

        let Some(rust_name) = rust_name.as_str() else {
            return Err(ValidationError::InvalidSatayEnumVariantName {
                context: context.to_owned(),
                wire_name: wire_name.clone(),
            });
        };
        let rust_name = variant_ident(rust_name);
        if rust_name != "Unknown" && !explicit_names.insert(rust_name.clone()) {
            return Err(ValidationError::DuplicateSatayEnumVariantName {
                context: context.to_owned(),
                rust_name,
            });
        }
        mappings.insert(wire_name.clone(), rust_name);
    }

    Ok(mappings)
}

fn parse_struct_fields(
    schema_name: &str,
    schema: &Map<String, Value>,
    registry: &mut TypeRegistry,
) -> Result<Vec<Field>, ValidationError> {
    let context = format!("schema `{schema_name}`");
    let required = parse_required_set(schema, &context)?;
    reject_keyword(schema, "minProperties", &context)?;
    reject_keyword(schema, "maxProperties", &context)?;
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| ValidationError::MissingObjectProperties {
            schema: schema_name.to_owned(),
        })?;

    let mut used = BTreeSet::new();
    let mut fields = Vec::with_capacity(properties.len());
    for (wire_name, property_schema) in properties {
        let rust_name = unique_ident(field_ident(wire_name), &mut used);
        let ty = parse_type_ref(
            property_schema,
            &format!("property `{schema_name}.{wire_name}`"),
            registry,
            Some(&format!("{schema_name} {wire_name}")),
        )?;
        let treat_error_as_none = parse_satay_treat_error_as_none(
            property_schema,
            &format!("property `{schema_name}.{wire_name}`"),
        )?;
        fields.push(Field {
            wire_name: wire_name.clone(),
            rust_name,
            ty,
            required: required.contains(wire_name),
            treat_error_as_none,
        });
    }

    Ok(fields)
}

fn parse_required_set(
    schema: &Map<String, Value>,
    context: &str,
) -> Result<BTreeSet<String>, ValidationError> {
    let Some(required) = schema.get("required") else {
        return Ok(BTreeSet::new());
    };
    let required = required
        .as_array()
        .ok_or_else(|| ValidationError::NonArrayRequired {
            context: context.to_owned(),
        })?;
    let mut set = BTreeSet::new();
    for value in required {
        let Some(name) = value.as_str() else {
            return Err(ValidationError::NonStringRequiredField {
                context: context.to_owned(),
            });
        };
        set.insert(name.to_owned());
    }
    Ok(set)
}

fn parse_type_ref(
    schema: &Value,
    context: &str,
    registry: &mut TypeRegistry,
    type_name_hint: Option<&str>,
) -> Result<TypeRef, ValidationError> {
    if let Some(reference) = schema_ref(schema) {
        return Ok(TypeRef::Named(schema_ref_type_name(reference)?));
    }

    let schema = object(schema, context)?;
    reject_composition(schema, context)?;

    let nullable = schema
        .get("nullable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let base = parse_type_ref_base(schema, context, registry, type_name_hint)?;
    let validation = parse_validation(schema, &base, context)?;
    let ty = if let Some(validation) = validation {
        registry.constrained_ref(type_name_hint.unwrap_or(context), base, validation)
    } else {
        base
    };

    if nullable {
        Ok(TypeRef::Nullable(Box::new(ty)))
    } else {
        Ok(ty)
    }
}

fn parse_type_ref_base(
    schema: &Map<String, Value>,
    context: &str,
    registry: &mut TypeRegistry,
    type_name_hint: Option<&str>,
) -> Result<TypeRef, ValidationError> {
    if let Some(parse_as) = parse_satay_parse_as(schema, context)? {
        let schema_type = schema.get("type").and_then(Value::as_str);
        match (schema_type, parse_as) {
            (Some("string"), parse_as) => return Ok(TypeRef::ParsedString(parse_as)),
            (Some("integer"), ParseAs::Bool) => return Ok(TypeRef::ParsedInteger(parse_as)),
            _ => {
                return Err(ValidationError::SatayParseAsRequiresString {
                    context: context.to_owned(),
                    parse_as: satay_parse_as_wire(parse_as).to_owned(),
                    kind: schema_type.unwrap_or("missing").to_owned(),
                });
            }
        }
    }

    if schema.contains_key("enum") {
        let mut variants = parse_string_enum(schema, context)?;
        let default_empty_variant = variant_ident("");
        variants.retain(|v| !v.wire_name.is_empty() || v.rust_name != default_empty_variant);
        if variants.is_empty() {
            return Ok(TypeRef::String);
        }
        let name_hint = type_name_hint.unwrap_or(context);
        return Ok(registry.inline_enum_ref(name_hint, variants));
    }

    match schema.get("type").and_then(Value::as_str) {
        Some("string") => Ok(TypeRef::String),
        Some("integer") => match schema.get("format").and_then(Value::as_str) {
            Some("int32") => Ok(TypeRef::I32),
            Some("int64") | None => Ok(TypeRef::I64),
            Some(format) => Err(ValidationError::UnsupportedIntegerFormat {
                context: context.to_owned(),
                format: format.to_owned(),
            }),
        },
        Some("number") => match schema.get("format").and_then(Value::as_str) {
            Some("float") => Ok(TypeRef::F32),
            Some("double") | None => Ok(TypeRef::F64),
            Some(format) => Err(ValidationError::UnsupportedNumberFormat {
                context: context.to_owned(),
                format: format.to_owned(),
            }),
        },
        Some("boolean") => Ok(TypeRef::Bool),
        Some("array") => {
            let items = schema
                .get("items")
                .ok_or_else(|| ValidationError::MissingArrayItems {
                    context: context.to_owned(),
                })?;
            let item_name_hint = type_name_hint.map(|name| format!("{name} item"));
            Ok(TypeRef::Array(Box::new(parse_type_ref(
                items,
                &format!("{context} items"),
                registry,
                item_name_hint.as_deref(),
            )?)))
        }
        Some("object") | None if schema.contains_key("properties") => {
            Err(ValidationError::InlineObjectSchema {
                context: context.to_owned(),
            })
        }
        Some("object") => Err(ValidationError::UnsupportedMapObjectSchema {
            context: context.to_owned(),
        }),
        Some(kind) => Err(ValidationError::UnsupportedSchemaType {
            context: context.to_owned(),
            kind: kind.to_owned(),
        }),
        None => Err(ValidationError::MissingSchemaType {
            context: context.to_owned(),
        }),
    }
}

fn parse_validation(
    schema: &Map<String, Value>,
    base: &TypeRef,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    match base {
        TypeRef::String => parse_string_validation(schema, context),
        TypeRef::I32 | TypeRef::I64 => parse_integer_validation(schema, base, context),
        TypeRef::F32 | TypeRef::F64 => parse_number_validation(schema, base, context),
        TypeRef::Array(_) => parse_array_validation(schema, context),
        TypeRef::ParsedString(_)
        | TypeRef::ParsedInteger(_)
        | TypeRef::Bool
        | TypeRef::Named(_)
        | TypeRef::Constrained { .. }
        | TypeRef::Nullable(_) => Ok(None),
    }
}

fn parse_satay_parse_as(
    schema: &Map<String, Value>,
    context: &str,
) -> Result<Option<ParseAs>, ValidationError> {
    let Some(satay) = optional_object(schema, "x-satay", context)? else {
        return Ok(None);
    };
    let Some(value) = satay.get("parse-as") else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(ValidationError::InvalidSatayParseAs {
            context: context.to_owned(),
        });
    };
    ParseAs::from_wire(value)
        .map(Some)
        .ok_or_else(|| ValidationError::UnsupportedSatayParseAs {
            context: context.to_owned(),
            parse_as: value.to_owned(),
        })
}

fn parse_satay_treat_error_as_none(schema: &Value, context: &str) -> Result<bool, ValidationError> {
    let Some(obj) = schema.as_object() else {
        return Ok(false);
    };
    let Some(satay) = optional_object(obj, "x-satay", context)? else {
        return Ok(false);
    };
    let Some(value) = satay.get("treat-error-as-none") else {
        return Ok(false);
    };
    value
        .as_bool()
        .ok_or_else(|| ValidationError::InvalidBooleanKeyword {
            context: context.to_owned(),
            keyword: "treat-error-as-none",
        })
}

fn satay_parse_as_wire(parse_as: ParseAs) -> &'static str {
    match parse_as {
        ParseAs::U8 => "u8",
        ParseAs::U16 => "u16",
        ParseAs::U32 => "u32",
        ParseAs::U64 => "u64",
        ParseAs::I8 => "i8",
        ParseAs::I16 => "i16",
        ParseAs::I32 => "i32",
        ParseAs::I64 => "i64",
        ParseAs::F32 => "f32",
        ParseAs::F64 => "f64",
        ParseAs::Bool => "bool",
        ParseAs::OffsetDateTime => "offset-datetime",
    }
}

fn parse_string_validation(
    schema: &Map<String, Value>,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    let pattern = schema
        .get("pattern")
        .and_then(Value::as_str)
        .map(|s| s.to_owned());

    let min_length = optional_u64_keyword(schema, "minLength", context)?;
    let max_length = optional_u64_keyword(schema, "maxLength", context)?;
    if let (Some(min_length), Some(max_length)) = (min_length, max_length)
        && min_length > max_length
    {
        return Err(ValidationError::InvalidStringLengthBounds {
            context: context.to_owned(),
            min_length,
            max_length,
        });
    }

    if pattern.is_some() || min_length.is_some() || max_length.is_some() {
        Ok(Some(Validation::String {
            min_length,
            max_length,
            pattern,
        }))
    } else {
        Ok(None)
    }
}

fn parse_integer_validation(
    schema: &Map<String, Value>,
    base: &TypeRef,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    reject_keyword(schema, "multipleOf", context)?;
    let minimum = optional_integer_limit(schema, "minimum", "exclusiveMinimum", context)?;
    let maximum = optional_integer_limit(schema, "maximum", "exclusiveMaximum", context)?;
    let (minimum, maximum) = normalize_integer_limits(minimum, maximum, base, context)?;

    if minimum.is_some() || maximum.is_some() {
        Ok(Some(Validation::Integer { minimum, maximum }))
    } else {
        Ok(None)
    }
}

fn parse_number_validation(
    schema: &Map<String, Value>,
    base: &TypeRef,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    reject_keyword(schema, "multipleOf", context)?;
    let minimum = optional_float_limit(schema, "minimum", "exclusiveMinimum", context)?;
    let maximum = optional_float_limit(schema, "maximum", "exclusiveMaximum", context)?;
    let (minimum, maximum) = normalize_float_limits(minimum, maximum, base, context)?;

    if minimum.is_some() || maximum.is_some() {
        Ok(Some(Validation::Number { minimum, maximum }))
    } else {
        Ok(None)
    }
}

fn parse_array_validation(
    schema: &Map<String, Value>,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    if schema
        .get("uniqueItems")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(ValidationError::UniqueItemsUnsupported {
            context: context.to_owned(),
        });
    }

    let min_items = optional_u64_keyword(schema, "minItems", context)?;
    let max_items = optional_u64_keyword(schema, "maxItems", context)?;
    if let (Some(min_items), Some(max_items)) = (min_items, max_items)
        && min_items > max_items
    {
        return Err(ValidationError::InvalidArrayLengthBounds {
            context: context.to_owned(),
            min_items,
            max_items,
        });
    }

    if min_items.is_some() || max_items.is_some() {
        Ok(Some(Validation::Array {
            min_items,
            max_items,
        }))
    } else {
        Ok(None)
    }
}

fn reject_keyword(
    schema: &Map<String, Value>,
    keyword: &'static str,
    context: &str,
) -> Result<(), ValidationError> {
    if schema.contains_key(keyword) {
        return Err(ValidationError::UnsupportedKeyword {
            context: context.to_owned(),
            keyword,
        });
    }
    Ok(())
}

fn optional_u64_keyword(
    schema: &Map<String, Value>,
    keyword: &'static str,
    context: &str,
) -> Result<Option<u64>, ValidationError> {
    let Some(value) = schema.get(keyword) else {
        return Ok(None);
    };
    match value.as_u64() {
        Some(value) => Ok(Some(value)),
        None => Err(ValidationError::InvalidNonNegativeIntegerKeyword {
            context: context.to_owned(),
            keyword,
        }),
    }
}

fn optional_bool_keyword(
    schema: &Map<String, Value>,
    keyword: &'static str,
    context: &str,
) -> Result<Option<bool>, ValidationError> {
    let Some(value) = schema.get(keyword) else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| ValidationError::InvalidBooleanKeyword {
            context: context.to_owned(),
            keyword,
        })
}

fn optional_integer_limit(
    schema: &Map<String, Value>,
    keyword: &'static str,
    exclusive_keyword: &'static str,
    context: &str,
) -> Result<Option<IntegerLimit>, ValidationError> {
    let exclusive = optional_bool_keyword(schema, exclusive_keyword, context)?;
    let Some(value) = schema.get(keyword) else {
        if exclusive.is_some() {
            return Err(ValidationError::ExclusiveLimitRequiresBound {
                context: context.to_owned(),
                exclusive_keyword,
                keyword,
            });
        }
        return Ok(None);
    };
    let value = json_integer(value, &format!("{context}.{keyword}"))?;
    Ok(Some(IntegerLimit {
        value,
        exclusive: exclusive.unwrap_or(false),
    }))
}

fn optional_float_limit(
    schema: &Map<String, Value>,
    keyword: &'static str,
    exclusive_keyword: &'static str,
    context: &str,
) -> Result<Option<FloatLimit>, ValidationError> {
    let exclusive = optional_bool_keyword(schema, exclusive_keyword, context)?;
    let Some(value) = schema.get(keyword) else {
        if exclusive.is_some() {
            return Err(ValidationError::ExclusiveLimitRequiresBound {
                context: context.to_owned(),
                exclusive_keyword,
                keyword,
            });
        }
        return Ok(None);
    };
    let value = value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| ValidationError::InvalidFiniteNumberKeyword {
            context: context.to_owned(),
            keyword,
        })?;
    Ok(Some(FloatLimit {
        value,
        exclusive: exclusive.unwrap_or(false),
    }))
}

fn json_integer(value: &Value, context: &str) -> Result<i128, ValidationError> {
    let Some(number) = value.as_number() else {
        return Err(ValidationError::ExpectedInteger {
            context: context.to_owned(),
        });
    };
    if let Some(value) = number.as_i64() {
        return Ok(i128::from(value));
    }
    if let Some(value) = number.as_u64() {
        return Ok(i128::from(value));
    }
    let Some(value) = number.as_f64() else {
        return Err(ValidationError::ExpectedInteger {
            context: context.to_owned(),
        });
    };
    if !value.is_finite() || value.fract() != 0.0 {
        return Err(ValidationError::ExpectedInteger {
            context: context.to_owned(),
        });
    }
    Ok(value as i128)
}

fn normalize_integer_limits(
    minimum: Option<IntegerLimit>,
    maximum: Option<IntegerLimit>,
    base: &TypeRef,
    context: &str,
) -> Result<(Option<IntegerLimit>, Option<IntegerLimit>), ValidationError> {
    let (type_min, type_max) = match base {
        TypeRef::I32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        TypeRef::I64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        _ => unreachable!("integer validation is only parsed for integer types"),
    };

    let effective_min = minimum
        .map(effective_integer_min)
        .transpose()?
        .unwrap_or(type_min);
    let effective_max = maximum
        .map(effective_integer_max)
        .transpose()?
        .unwrap_or(type_max);

    if effective_min > effective_max {
        return Err(ValidationError::EmptyIntegerBounds {
            context: context.to_owned(),
        });
    }

    let minimum = minimum.filter(|_| effective_min > type_min);
    let maximum = maximum.filter(|_| effective_max < type_max);
    Ok((minimum, maximum))
}

fn effective_integer_min(limit: IntegerLimit) -> Result<i128, ValidationError> {
    if limit.exclusive {
        limit
            .value
            .checked_add(1)
            .ok_or(ValidationError::ExclusiveIntegerMinimumOverflow)
    } else {
        Ok(limit.value)
    }
}

fn effective_integer_max(limit: IntegerLimit) -> Result<i128, ValidationError> {
    if limit.exclusive {
        limit
            .value
            .checked_sub(1)
            .ok_or(ValidationError::ExclusiveIntegerMaximumOverflow)
    } else {
        Ok(limit.value)
    }
}

fn normalize_float_limits(
    minimum: Option<FloatLimit>,
    maximum: Option<FloatLimit>,
    base: &TypeRef,
    context: &str,
) -> Result<(Option<FloatLimit>, Option<FloatLimit>), ValidationError> {
    let (type_min, type_max) = match base {
        TypeRef::F32 => (f64::from(f32::MIN), f64::from(f32::MAX)),
        TypeRef::F64 => (f64::MIN, f64::MAX),
        _ => unreachable!("number validation is only parsed for number types"),
    };

    let effective_min = minimum.map(|limit| limit.value).unwrap_or(type_min);
    let effective_max = maximum.map(|limit| limit.value).unwrap_or(type_max);
    if effective_min > effective_max
        || (effective_min == effective_max
            && minimum.is_some_and(|limit| limit.exclusive)
            && maximum.is_some_and(|limit| limit.exclusive))
    {
        return Err(ValidationError::EmptyNumberBounds {
            context: context.to_owned(),
        });
    }

    let minimum = minimum.filter(|limit| limit.value > type_min);
    let maximum = maximum.filter(|limit| limit.value < type_max);
    Ok((minimum, maximum))
}

fn parse_operations(
    document: &Value,
    registry: &mut TypeRegistry,
) -> Result<Vec<Operation>, ValidationError> {
    let root = object(document, "OpenAPI document")?;
    let paths = root
        .get("paths")
        .and_then(Value::as_object)
        .ok_or(ValidationError::MissingPaths)?;

    let mut operations = Vec::new();
    for (path, path_item) in paths {
        let path_item = resolve_reference(document, path_item, &format!("path item `{path}`"))?;
        let path_item = object(path_item, &format!("path item `{path}`"))?;
        let path_parameter_prefix = type_ident(&format!("{path} parameter"));
        let path_parameters = parse_parameter_list(
            document,
            path_item.get("parameters"),
            &format!("path item `{path}` parameters"),
            registry,
            &path_parameter_prefix,
        )?;

        for (method_name, operation) in path_item {
            let Some(method) = HttpMethod::from_key(method_name) else {
                continue;
            };
            operations.push(parse_operation(
                document,
                method,
                path,
                &path_parameters,
                operation,
                registry,
            )?);
        }
    }

    Ok(operations)
}

fn parse_operation(
    document: &Value,
    method: HttpMethod,
    path: &str,
    path_parameters: &[Parameter],
    operation: &Value,
    registry: &mut TypeRegistry,
) -> Result<Operation, ValidationError> {
    let operation = object(operation, &format!("{} {path}", method.operation_prefix()))?;
    let operation_id = operation
        .get("operationId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| inferred_operation_id(method, path));
    let fn_name = function_ident(&operation_id);
    let type_prefix = type_ident(&operation_id);

    let mut parameters = path_parameters.to_vec();
    for parameter in parse_parameter_list(
        document,
        operation.get("parameters"),
        &format!("operation `{operation_id}` parameters"),
        registry,
        &type_prefix,
    )? {
        upsert_parameter(&mut parameters, parameter);
    }
    deduplicate_parameter_fields(&mut parameters);

    let path_segments = parse_path_segments(path)?;
    validate_path_parameters(path, &path_segments, &parameters)?;

    let request_body = parse_request_body(
        document,
        operation.get("requestBody"),
        &format!("operation `{operation_id}` requestBody"),
        &parameters,
        registry,
        &type_prefix,
    )?;

    let responses = parse_responses(
        document,
        operation
            .get("responses")
            .ok_or_else(|| ValidationError::MissingOperationResponses {
                operation_id: operation_id.clone(),
            })?,
        &format!("operation `{operation_id}` responses"),
        registry,
        &type_prefix,
    )?;

    Ok(Operation {
        fn_name,
        input_name: format!("{type_prefix}Input"),
        response_name: format!("{type_prefix}Response"),
        method,
        path: path.to_owned(),
        path_segments,
        parameters,
        request_body,
        responses,
    })
}

fn parse_parameter_list(
    document: &Value,
    parameters: Option<&Value>,
    context: &str,
    registry: &mut TypeRegistry,
    type_prefix: &str,
) -> Result<Vec<Parameter>, ValidationError> {
    let Some(parameters) = parameters else {
        return Ok(vec![]);
    };
    let parameters = parameters
        .as_array()
        .ok_or_else(|| ValidationError::ExpectedArray {
            context: context.to_owned(),
        })?;

    let mut parsed = Vec::with_capacity(parameters.len());
    for parameter in parameters {
        parsed.push(parse_parameter(
            document,
            parameter,
            context,
            registry,
            type_prefix,
        )?);
    }
    Ok(parsed)
}

fn parse_parameter(
    document: &Value,
    parameter: &Value,
    context: &str,
    registry: &mut TypeRegistry,
    type_prefix: &str,
) -> Result<Parameter, ValidationError> {
    let parameter = resolve_reference(document, parameter, context)?;
    let parameter = object(parameter, context)?;
    let wire_name = required_str(parameter, "name", context)?.to_owned();
    let location = match required_str(parameter, "in", context)? {
        "path" => ParameterLocation::Path,
        "query" => ParameterLocation::Query,
        "header" => ParameterLocation::Header,
        other => {
            return Err(ValidationError::UnsupportedParameterLocation {
                context: context.to_owned(),
                wire_name: wire_name.clone(),
                location: other.to_owned(),
            });
        }
    };

    if parameter.contains_key("content") {
        return Err(ValidationError::ContentParameterUnsupported {
            context: context.to_owned(),
            wire_name: wire_name.clone(),
        });
    }

    let schema =
        parameter
            .get("schema")
            .ok_or_else(|| ValidationError::MissingParameterSchema {
                context: context.to_owned(),
                wire_name: wire_name.clone(),
            })?;
    let ty = parse_type_ref(
        schema,
        &format!("parameter `{wire_name}`"),
        registry,
        Some(&format!("{type_prefix} {wire_name} parameter")),
    )?;
    if ty.is_nullable() {
        return Err(ValidationError::NullableParameterUnsupported {
            wire_name: wire_name.clone(),
        });
    }
    if location == ParameterLocation::Path && is_array_type(ty.non_nullable()) {
        return Err(ValidationError::ArrayPathParameterUnsupported {
            wire_name: wire_name.clone(),
        });
    }
    if location == ParameterLocation::Header && is_array_type(ty.non_nullable()) {
        return Err(ValidationError::ArrayHeaderParameterUnsupported {
            wire_name: wire_name.clone(),
        });
    }

    let required = match location {
        ParameterLocation::Path => {
            if parameter.get("required").and_then(Value::as_bool) != Some(true) {
                return Err(ValidationError::PathParameterNotRequired {
                    wire_name: wire_name.clone(),
                });
            }
            true
        }
        ParameterLocation::Query | ParameterLocation::Header => parameter
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };

    Ok(Parameter {
        location,
        wire_name: wire_name.clone(),
        rust_name: field_ident(&wire_name),
        ty,
        required,
    })
}

fn upsert_parameter(parameters: &mut Vec<Parameter>, parameter: Parameter) {
    if let Some(existing) = parameters.iter_mut().find(|existing| {
        existing.location == parameter.location && existing.wire_name == parameter.wire_name
    }) {
        *existing = parameter;
    } else {
        parameters.push(parameter);
    }
}

fn deduplicate_parameter_fields(parameters: &mut [Parameter]) {
    let mut used = BTreeSet::new();
    for parameter in parameters {
        parameter.rust_name = unique_ident(parameter.rust_name.clone(), &mut used);
    }
}

fn parse_request_body(
    document: &Value,
    request_body: Option<&Value>,
    context: &str,
    parameters: &[Parameter],
    registry: &mut TypeRegistry,
    type_prefix: &str,
) -> Result<Option<RequestBody>, ValidationError> {
    let Some(request_body) = request_body else {
        return Ok(None);
    };
    let request_body = resolve_reference(document, request_body, context)?;
    let request_body = object(request_body, context)?;
    let content = request_body
        .get("content")
        .and_then(Value::as_object)
        .ok_or_else(|| ValidationError::MissingContent {
            context: context.to_owned(),
        })?;
    let (content_type, media_type) =
        json_media_type(content).ok_or_else(|| ValidationError::MissingJsonContent {
            context: context.to_owned(),
        })?;
    let media_type = object(media_type, context)?;
    let schema = media_type
        .get("schema")
        .ok_or_else(|| ValidationError::MissingJsonSchema {
            context: context.to_owned(),
        })?;

    let mut used = parameters
        .iter()
        .map(|parameter| parameter.rust_name.clone())
        .collect::<BTreeSet<_>>();
    let field_name = unique_ident("body".to_owned(), &mut used);

    Ok(Some(RequestBody {
        field_name,
        content_type: content_type.to_owned(),
        ty: parse_type_ref(
            schema,
            context,
            registry,
            Some(&format!("{type_prefix} request body")),
        )?,
        required: request_body
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }))
}

fn parse_responses(
    document: &Value,
    responses: &Value,
    context: &str,
    registry: &mut TypeRegistry,
    type_prefix: &str,
) -> Result<Vec<ResponseCase>, ValidationError> {
    let responses = object(responses, context)?;
    let mut cases = Vec::new();
    for (status, response) in responses {
        if status == "default" {
            let response = resolve_reference(document, response, &format!("{context} default"))?;
            let response = object(response, &format!("{context} default"))?;
            if response
                .get("content")
                .and_then(Value::as_object)
                .is_some_and(|content| !content.is_empty())
            {
                return Err(ValidationError::DefaultResponseBodyUnsupported {
                    context: context.to_owned(),
                });
            }
            continue;
        }
        let status_code =
            status
                .parse::<u16>()
                .map_err(|_| ValidationError::InvalidStatusCode {
                    context: context.to_owned(),
                    status: status.to_owned(),
                })?;
        if !(100..=599).contains(&status_code) {
            return Err(ValidationError::OutOfRangeStatusCode {
                context: context.to_owned(),
                status_code,
            });
        }

        let response = resolve_reference(document, response, &format!("{context} {status}"))?;
        let response = object(response, &format!("{context} {status}"))?;
        let body = match response.get("content").and_then(Value::as_object) {
            Some(content) if content.is_empty() => None,
            Some(content) => {
                let (_, media_type) = json_media_type(content).ok_or_else(|| {
                    ValidationError::MissingResponseJsonContent {
                        context: context.to_owned(),
                        status: status.to_owned(),
                    }
                })?;
                let media_type = object(media_type, &format!("{context} {status}"))?;
                match media_type.get("schema") {
                    Some(schema) => Some(parse_type_ref(
                        schema,
                        &format!("{context} {status} schema"),
                        registry,
                        Some(&format!("{type_prefix} response {status}")),
                    )?),
                    None => None,
                }
            }
            None => None,
        };

        cases.push(ResponseCase {
            status: status_code,
            variant_name: response_variant_ident(status_code),
            body,
        });
    }

    cases.sort_by_key(|case| case.status);
    Ok(cases)
}

fn parse_path_segments(path: &str) -> Result<Vec<PathSegment>, ValidationError> {
    let mut segments = Vec::new();
    let mut rest = path;

    loop {
        let Some(open) = rest.find('{') else {
            if !rest.is_empty() {
                segments.push(PathSegment::Literal(rest.to_owned()));
            }
            return Ok(segments);
        };

        if open > 0 {
            segments.push(PathSegment::Literal(rest[..open].to_owned()));
        }
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('}') else {
            return Err(ValidationError::UnclosedPathParameter {
                path: path.to_owned(),
            });
        };
        let name = &after_open[..close];
        if name.is_empty() {
            return Err(ValidationError::EmptyPathParameter {
                path: path.to_owned(),
            });
        }
        segments.push(PathSegment::Parameter(name.to_owned()));
        rest = &after_open[close + 1..];
    }
}

fn validate_path_parameters(
    path: &str,
    path_segments: &[PathSegment],
    parameters: &[Parameter],
) -> Result<(), ValidationError> {
    let declared = parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|parameter| parameter.wire_name.as_str())
        .collect::<BTreeSet<_>>();

    let mut placeholders = BTreeSet::new();
    for segment in path_segments {
        if let PathSegment::Parameter(name) = segment {
            placeholders.insert(name.as_str());
            if !declared.contains(name.as_str()) {
                return Err(ValidationError::UndeclaredPathParameter {
                    path: path.to_owned(),
                    name: name.to_owned(),
                });
            }
        }
    }

    for name in declared {
        if !placeholders.contains(name) {
            return Err(ValidationError::UnusedPathParameter {
                path: path.to_owned(),
                name: name.to_owned(),
            });
        }
    }

    Ok(())
}

fn inferred_operation_id(method: HttpMethod, path: &str) -> String {
    let mut parts = Vec::new();
    parts.push(method.operation_prefix().to_owned());
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        if let Some(name) = segment
            .strip_prefix('{')
            .and_then(|part| part.strip_suffix('}'))
        {
            parts.push("by".to_owned());
            parts.push(name.to_owned());
        } else {
            parts.push(segment.to_owned());
        }
    }
    parts.join("_")
}

fn schema_ref(value: &Value) -> Option<&str> {
    value
        .as_object()
        .and_then(|object| object.get("$ref"))
        .and_then(Value::as_str)
}

fn schema_ref_type_name(reference: &str) -> Result<String, ValidationError> {
    let name = local_ref_name(reference, "schemas")?;
    Ok(type_ident(&name))
}

fn resolve_reference<'a>(
    document: &'a Value,
    value: &'a Value,
    context: &str,
) -> Result<&'a Value, ValidationError> {
    let Some(reference) = value
        .as_object()
        .and_then(|object| object.get("$ref"))
        .and_then(Value::as_str)
    else {
        return Ok(value);
    };

    resolve_json_pointer(document, reference).map_err(|source| ValidationError::ResolveReference {
        reference: reference.to_owned(),
        context: context.to_owned(),
        source: Box::new(source),
    })
}

fn resolve_json_pointer<'a>(
    document: &'a Value,
    reference: &str,
) -> Result<&'a Value, ValidationError> {
    let Some(pointer) = reference.strip_prefix('#') else {
        return Err(ValidationError::NonLocalReference);
    };
    if pointer.is_empty() {
        return Ok(document);
    }
    if !pointer.starts_with('/') {
        return Err(ValidationError::InvalidLocalReference);
    }

    let mut current = document;
    for token in pointer[1..].split('/') {
        let token = json_pointer_unescape(token);
        current = current
            .as_object()
            .and_then(|object| object.get(&token))
            .ok_or_else(|| ValidationError::MissingJsonPointerToken {
                token: token.clone(),
            })?;
    }
    Ok(current)
}

fn local_ref_name(reference: &str, section: &'static str) -> Result<String, ValidationError> {
    let prefix = format!("#/components/{section}/");
    let Some(name) = reference.strip_prefix(&prefix) else {
        return Err(ValidationError::InvalidComponentReference {
            reference: reference.to_owned(),
            section,
        });
    };
    Ok(json_pointer_unescape(name))
}

fn json_pointer_unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>, ValidationError> {
    value
        .as_object()
        .ok_or_else(|| ValidationError::ExpectedObject {
            context: context.to_owned(),
        })
}

fn optional_object<'a>(
    object: &'a Map<String, Value>,
    field: &'static str,
    context: &str,
) -> Result<Option<&'a Map<String, Value>>, ValidationError> {
    match object.get(field) {
        Some(value) => {
            value
                .as_object()
                .map(Some)
                .ok_or_else(|| ValidationError::ExpectedObjectField {
                    context: context.to_owned(),
                    field,
                })
        }
        None => Ok(None),
    }
}

fn required_str<'a>(
    object: &'a Map<String, Value>,
    field: &'static str,
    context: &str,
) -> Result<&'a str, ValidationError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| ValidationError::MissingStringField {
            context: context.to_owned(),
            field,
        })
}

fn reject_composition(schema: &Map<String, Value>, context: &str) -> Result<(), ValidationError> {
    for keyword in ["oneOf", "anyOf", "allOf"] {
        if schema.contains_key(keyword) {
            return Err(ValidationError::UnsupportedComposition {
                context: context.to_owned(),
                keyword,
            });
        }
    }
    Ok(())
}

fn json_media_type(content: &Map<String, Value>) -> Option<(&str, &Value)> {
    content
        .get_key_value("application/json")
        .map(|(media_type, value)| (media_type.as_str(), value))
        .or_else(|| {
            content
                .iter()
                .find(|(media_type, _)| is_json_media_type(media_type))
                .map(|(media_type, value)| (media_type.as_str(), value))
        })
}

fn is_json_media_type(value: &str) -> bool {
    let media_type = value.split(';').next().unwrap_or(value).trim();
    if media_type.eq_ignore_ascii_case("application/json") {
        return true;
    }
    let Some((_, subtype)) = media_type.rsplit_once('/') else {
        return false;
    };
    ends_with_ignore_ascii_case(subtype, "+json")
}

fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    let value = value.as_bytes();
    let suffix = suffix.as_bytes();
    value.len() >= suffix.len() && value[value.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_valid(spec: &str) -> Api {
        let document = parse_document(spec).expect("document parses");
        parse_api(&document).expect("OpenAPI validates")
    }

    fn parse_invalid(spec: &str) -> ValidationError {
        let document = parse_document(spec).expect("document parses");
        parse_api(&document).expect_err("OpenAPI must be rejected")
    }

    fn component<'a>(api: &'a Api, rust_name: &str) -> &'a Component {
        api.components
            .iter()
            .find(|component| component.rust_name == rust_name)
            .unwrap_or_else(|| panic!("missing component {rust_name}"))
    }

    fn field<'a>(fields: &'a [Field], wire_name: &str) -> &'a Field {
        fields
            .iter()
            .find(|field| field.wire_name == wire_name)
            .unwrap_or_else(|| panic!("missing field {wire_name}"))
    }

    fn parameter<'a>(operation: &'a Operation, wire_name: &str) -> &'a Parameter {
        operation
            .parameters
            .iter()
            .find(|parameter| parameter.wire_name == wire_name)
            .unwrap_or_else(|| panic!("missing parameter {wire_name}"))
    }

    fn assert_literal_segment(segment: &PathSegment, expected: &str) {
        match segment {
            PathSegment::Literal(actual) => assert_eq!(actual, expected),
            other => panic!("expected literal path segment {expected:?}, got {other:?}"),
        }
    }

    fn assert_parameter_segment(segment: &PathSegment, expected: &str) {
        match segment {
            PathSegment::Parameter(actual) => assert_eq!(actual, expected),
            other => panic!("expected parameter path segment {expected:?}, got {other:?}"),
        }
    }

    #[test]
    fn parses_components_operations_and_json_media_types_into_ir() {
        let api = parse_valid(
            r#"
openapi: 3.0.3
servers:
  - url: https://api.example.test/v1
paths:
  /users/{userId}:
    parameters:
      - name: userId
        in: path
        required: true
        schema:
          type: string
      - name: body
        in: query
        schema:
          type: boolean
    get:
      operationId: getUser
      parameters:
        - name: body
          in: query
          required: false
          schema:
            type: integer
            format: int32
        - name: includeDetails
          in: query
          required: true
          schema:
            type: boolean
      requestBody:
        required: true
        content:
          application/vnd.acme.user+json; charset=utf-8:
            schema:
              $ref: '#/components/schemas/UpdateUserRequest'
      responses:
        '404':
          description: Missing
        '200':
          description: Found
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
components:
  securitySchemes:
    accountKeyAuth:
      type: apiKey
      in: header
      name: AccountKey
    queryKeyAuth:
      type: apiKey
      in: query
      name: api_key
    bearerAuth:
      type: http
      scheme: bearer
  schemas:
    UpdateUserRequest:
      type: object
      required:
        - name
      properties:
        name:
          type: string
    User:
      type: object
      required:
        - id
        - status
      properties:
        id:
          type: string
        status:
          type: string
          enum:
            - active
            - suspended
        age:
          type: integer
          format: int64
"#,
        );

        assert_eq!(api.components.len(), 3);
        assert!(api.constrained_types.is_empty());
        assert_eq!(api.server_url, "https://api.example.test/v1");
        assert_eq!(api.api_key_security_schemes.len(), 2);
        assert_eq!(
            api.api_key_security_schemes[0].location,
            ApiKeyLocation::Header
        );
        assert_eq!(api.api_key_security_schemes[0].wire_name, "AccountKey");
        assert_eq!(api.api_key_security_schemes[0].rust_name, "account_key");
        assert_eq!(
            api.api_key_security_schemes[1].location,
            ApiKeyLocation::Query
        );
        assert_eq!(api.api_key_security_schemes[1].wire_name, "api_key");
        assert_eq!(api.api_key_security_schemes[1].rust_name, "api_key");

        let update_user_request = component(&api, "UpdateUserRequest");
        match &update_user_request.kind {
            ComponentKind::Struct(fields) => {
                assert_eq!(fields.len(), 1);
                let name = field(fields, "name");
                assert_eq!(name.rust_name, "name");
                assert_eq!(name.ty, TypeRef::String);
                assert!(name.required);
            }
            other => panic!("expected UpdateUserRequest struct, got {other:?}"),
        }

        let user_status = component(&api, "UserStatus");
        match &user_status.kind {
            ComponentKind::Enum(variants) => {
                assert_eq!(variants.len(), 2);
                assert_eq!(variants[0].wire_name, "active");
                assert_eq!(variants[0].rust_name, "Active");
                assert_eq!(variants[1].wire_name, "suspended");
                assert_eq!(variants[1].rust_name, "Suspended");
            }
            other => panic!("expected UserStatus enum, got {other:?}"),
        }

        let user = component(&api, "User");
        match &user.kind {
            ComponentKind::Struct(fields) => {
                assert_eq!(fields.len(), 3);

                let id = field(fields, "id");
                assert_eq!(id.ty, TypeRef::String);
                assert!(id.required);

                let status = field(fields, "status");
                assert_eq!(status.ty, TypeRef::Named("UserStatus".to_owned()));
                assert!(status.required);

                let age = field(fields, "age");
                assert_eq!(age.ty, TypeRef::I64);
                assert!(!age.required);
            }
            other => panic!("expected User struct, got {other:?}"),
        }

        assert_eq!(api.operations.len(), 1);
        let operation = &api.operations[0];
        assert_eq!(operation.fn_name, "get_user");
        assert_eq!(operation.input_name, "GetUserInput");
        assert_eq!(operation.response_name, "GetUserResponse");
        assert_eq!(operation.method, HttpMethod::Get);
        assert_eq!(operation.path, "/users/{userId}");
        assert_eq!(operation.path_segments.len(), 2);
        assert_literal_segment(&operation.path_segments[0], "/users/");
        assert_parameter_segment(&operation.path_segments[1], "userId");

        assert_eq!(operation.parameters.len(), 3);
        let user_id = parameter(operation, "userId");
        assert_eq!(user_id.location, ParameterLocation::Path);
        assert_eq!(user_id.rust_name, "user_id");
        assert_eq!(user_id.ty, TypeRef::String);
        assert!(user_id.required);

        let body = parameter(operation, "body");
        assert_eq!(body.location, ParameterLocation::Query);
        assert_eq!(body.rust_name, "body");
        assert_eq!(body.ty, TypeRef::I32);
        assert!(!body.required);

        let include_details = parameter(operation, "includeDetails");
        assert_eq!(include_details.location, ParameterLocation::Query);
        assert_eq!(include_details.rust_name, "include_details");
        assert_eq!(include_details.ty, TypeRef::Bool);
        assert!(include_details.required);

        let request_body = operation.request_body.as_ref().expect("request body");
        assert_eq!(request_body.field_name, "body_2");
        assert_eq!(
            request_body.content_type,
            "application/vnd.acme.user+json; charset=utf-8"
        );
        assert_eq!(
            request_body.ty,
            TypeRef::Named("UpdateUserRequest".to_owned())
        );
        assert!(request_body.required);

        assert_eq!(operation.responses.len(), 2);
        assert_eq!(operation.responses[0].status, 200);
        assert_eq!(operation.responses[0].variant_name, "Ok");
        assert_eq!(
            operation.responses[0].body,
            Some(TypeRef::Named("User".to_owned()))
        );
        assert_eq!(operation.responses[1].status, 404);
        assert_eq!(operation.responses[1].variant_name, "NotFound");
        assert_eq!(operation.responses[1].body, None);
    }

    #[test]
    fn lifts_inline_constraints_into_generated_types() {
        let api = parse_valid(
            r#"
openapi: 3.0.3
paths:
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
        - name: tag
          in: query
          schema:
            type: array
            minItems: 1
            items:
              type: string
              minLength: 2
      responses:
        '204':
          description: No content
components:
  schemas:
    Age:
      type: integer
      format: int32
      minimum: 0
      maximum: 130
    DisplayName:
      type: string
      nullable: true
      minLength: 1
"#,
        );

        let age = component(&api, "Age");
        match &age.kind {
            ComponentKind::Nutype(constrained) => {
                assert_eq!(constrained.rust_name, "Age");
                assert_eq!(constrained.inner, TypeRef::I32);
                match &constrained.validation {
                    Validation::Integer { minimum, maximum } => {
                        assert_eq!(
                            minimum,
                            &Some(IntegerLimit {
                                value: 0,
                                exclusive: false,
                            })
                        );
                        assert_eq!(
                            maximum,
                            &Some(IntegerLimit {
                                value: 130,
                                exclusive: false,
                            })
                        );
                    }
                    other => panic!("expected Age integer validation, got {other:?}"),
                }
            }
            other => panic!("expected Age nutype, got {other:?}"),
        }

        let display_name = component(&api, "DisplayName");
        match &display_name.kind {
            ComponentKind::Alias(TypeRef::Nullable(inner)) => match inner.as_ref() {
                TypeRef::Constrained { rust_name, inner } => {
                    assert_eq!(rust_name, "DisplayNameValue");
                    assert_eq!(inner.as_ref(), &TypeRef::String);
                }
                other => panic!("expected constrained nullable DisplayName, got {other:?}"),
            },
            other => panic!("expected DisplayName nullable alias, got {other:?}"),
        }

        let generated_names = api
            .constrained_types
            .iter()
            .map(|constrained| constrained.rust_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            generated_names,
            [
                "DisplayNameValue",
                "GetUserTagParameterItem",
                "GetUserTagParameter",
            ]
        );

        match &api.constrained_types[0].validation {
            Validation::String {
                min_length,
                max_length,
                pattern,
            } => {
                assert_eq!(*min_length, Some(1));
                assert_eq!(*max_length, None);
                assert_eq!(*pattern, None);
            }
            other => panic!("expected DisplayNameValue string validation, got {other:?}"),
        }

        match &api.constrained_types[1].validation {
            Validation::String {
                min_length,
                max_length,
                pattern,
            } => {
                assert_eq!(*min_length, Some(2));
                assert_eq!(*max_length, None);
                assert_eq!(*pattern, None);
            }
            other => panic!("expected tag item string validation, got {other:?}"),
        }

        match &api.constrained_types[2].validation {
            Validation::Array {
                min_items,
                max_items,
            } => {
                assert_eq!(*min_items, Some(1));
                assert_eq!(*max_items, None);
            }
            other => panic!("expected tag array validation, got {other:?}"),
        }

        let operation = &api.operations[0];
        let tag = parameter(operation, "tag");
        match &tag.ty {
            TypeRef::Constrained { rust_name, inner } => {
                assert_eq!(rust_name, "GetUserTagParameter");
                match inner.as_ref() {
                    TypeRef::Array(item) => match item.as_ref() {
                        TypeRef::Constrained { rust_name, inner } => {
                            assert_eq!(rust_name, "GetUserTagParameterItem");
                            assert_eq!(inner.as_ref(), &TypeRef::String);
                        }
                        other => panic!("expected constrained tag item, got {other:?}"),
                    },
                    other => panic!("expected constrained tag array, got {other:?}"),
                }
            }
            other => panic!("expected constrained tag parameter, got {other:?}"),
        }
    }

    #[test]
    fn parses_x_satay_parse_as_for_string_schemas() {
        let api = parse_valid(
            r#"
openapi: 3.0.3
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    Arrival:
      type: object
      required:
        - stop
        - latitude
        - visit
        - monitored
        - numericMonitored
        - estimatedArrival
      properties:
        stop:
          type: string
          minLength: 1
          x-satay:
            parse-as: u32
        latitude:
          type: string
          x-satay:
            parse-as: f64
        visit:
          type: string
          x-satay:
            parse-as: u8
        monitored:
          type: string
          x-satay:
            parse-as: bool
        numericMonitored:
          type: integer
          x-satay:
            parse-as: bool
        estimatedArrival:
          type: string
          x-satay:
            parse-as: offset-datetime
"#,
        );

        let arrival = component(&api, "Arrival");
        match &arrival.kind {
            ComponentKind::Struct(fields) => {
                assert_eq!(
                    field(fields, "stop").ty,
                    TypeRef::ParsedString(ParseAs::U32)
                );
                assert_eq!(
                    field(fields, "latitude").ty,
                    TypeRef::ParsedString(ParseAs::F64)
                );
                assert_eq!(
                    field(fields, "visit").ty,
                    TypeRef::ParsedString(ParseAs::U8)
                );
                assert_eq!(
                    field(fields, "monitored").ty,
                    TypeRef::ParsedString(ParseAs::Bool)
                );
                assert_eq!(
                    field(fields, "numericMonitored").ty,
                    TypeRef::ParsedInteger(ParseAs::Bool)
                );
                assert_eq!(
                    field(fields, "estimatedArrival").ty,
                    TypeRef::ParsedString(ParseAs::OffsetDateTime)
                );
            }
            other => panic!("expected Arrival struct, got {other:?}"),
        }
    }

    #[test]
    fn parses_x_satay_enum_variants() {
        let api = parse_valid(
            r#"
openapi: 3.0.3
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    VehicleType:
      type: string
      enum:
        - SD
        - DD
        - BD
        - ""
      x-satay:
        enum-variants:
          SD: SingleDecker
          DD: DoubleDecker
          BD: Bendy
          "": Unknown
    Arrival:
      type: object
      required:
        - type
      properties:
        type:
          type: string
          enum:
            - SD
            - DD
            - BD
            - ""
          x-satay:
            enum-variants:
              SD: SingleDecker
              DD: DoubleDecker
              BD: Bendy
              "": Unknown
"#,
        );

        let vehicle_type = component(&api, "VehicleType");
        match &vehicle_type.kind {
            ComponentKind::Enum(variants) => {
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[0].wire_name, "SD");
                assert_eq!(variants[0].rust_name, "SingleDecker");
                assert_eq!(variants[1].wire_name, "DD");
                assert_eq!(variants[1].rust_name, "DoubleDecker");
                assert_eq!(variants[2].wire_name, "BD");
                assert_eq!(variants[2].rust_name, "Bendy");
            }
            other => panic!("expected VehicleType enum, got {other:?}"),
        }

        let arrival_type = component(&api, "ArrivalType");
        match &arrival_type.kind {
            ComponentKind::Enum(variants) => {
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[0].rust_name, "SingleDecker");
                assert_eq!(variants[1].rust_name, "DoubleDecker");
                assert_eq!(variants[2].rust_name, "Bendy");
            }
            other => panic!("expected ArrivalType enum, got {other:?}"),
        }
    }

    #[test]
    fn rejects_x_satay_enum_variants_for_values_outside_enum() {
        let err = parse_invalid(
            r#"
openapi: 3.0.3
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/VehicleType'
components:
  schemas:
    VehicleType:
      type: string
      enum:
        - SD
      x-satay:
        enum-variants:
          DD: DoubleDecker
"#,
        );

        match err {
            ValidationError::UnknownSatayEnumVariantValue { context, wire_name } => {
                assert_eq!(context, "schema `VehicleType`");
                assert_eq!(wire_name, "DD");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn parses_x_satay_treat_error_as_none() {
        let api = parse_valid(
            r#"
openapi: 3.0.3
paths:
  /arrival:
    get:
      operationId: getArrival
      responses:
        '200':
          description: Arrival
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/Arrival'
components:
  schemas:
    Arrival:
      type: object
      required:
        - timing
      properties:
        timing:
          $ref: '#/components/schemas/Timing'
          x-satay:
            treat-error-as-none: true
        optionalTiming:
          $ref: '#/components/schemas/Timing'
    Timing:
      type: object
      required:
        - value
      properties:
        value:
          type: string
"#,
        );

        let arrival = component(&api, "Arrival");
        match &arrival.kind {
            ComponentKind::Struct(fields) => {
                let timing = field(fields, "timing");
                assert!(timing.treat_error_as_none);
                let optional_timing = field(fields, "optionalTiming");
                assert!(!optional_timing.treat_error_as_none);
            }
            other => panic!("expected Arrival struct, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_validation_bounds_before_rendering() {
        let err = parse_invalid(
            r#"
openapi: 3.0.3
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    Broken:
      type: string
      minLength: 4
      maxLength: 2
"#,
        );
        match err {
            ValidationError::InvalidStringLengthBounds {
                context,
                min_length,
                max_length,
            } => {
                assert_eq!(context, "schema `Broken`");
                assert_eq!(min_length, 4);
                assert_eq!(max_length, 2);
            }
            other => panic!("unexpected error: {other}"),
        }

        let err = parse_invalid(
            r#"
openapi: 3.0.3
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    Broken:
      type: integer
      format: int32
      exclusiveMinimum: true
"#,
        );
        match err {
            ValidationError::ExclusiveLimitRequiresBound {
                context,
                exclusive_keyword,
                keyword,
            } => {
                assert_eq!(context, "schema `Broken`");
                assert_eq!(exclusive_keyword, "exclusiveMinimum");
                assert_eq!(keyword, "minimum");
            }
            other => panic!("unexpected error: {other}"),
        }

        let err = parse_invalid(
            r#"
openapi: 3.0.3
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '204':
          description: No content
components:
  schemas:
    Broken:
      type: number
      minimum: 5
      maximum: 5
      exclusiveMinimum: true
      exclusiveMaximum: true
"#,
        );
        match err {
            ValidationError::EmptyNumberBounds { context } => {
                assert_eq!(context, "schema `Broken`");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn reports_reference_and_path_validation_errors() {
        let err = parse_invalid(
            r#"
openapi: 3.0.3
paths:
  /users/{userId}:
    get:
      operationId: getUser
      parameters:
        - name: accountId
          in: path
          required: true
          schema:
            type: string
      responses:
        '204':
          description: No content
"#,
        );
        match err {
            ValidationError::UndeclaredPathParameter { path, name } => {
                assert_eq!(path, "/users/{userId}");
                assert_eq!(name, "userId");
            }
            other => panic!("unexpected error: {other}"),
        }

        let err = parse_invalid(
            r##"
openapi: 3.0.3
paths:
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
        - $ref: '#/components/parameters/Missing'
      responses:
        '204':
          description: No content
components:
  parameters: {}
"##,
        );
        match err {
            ValidationError::ResolveReference {
                reference,
                context,
                source,
            } => {
                assert_eq!(reference, "#/components/parameters/Missing");
                assert_eq!(context, "operation `getUser` parameters");
                match *source {
                    ValidationError::MissingJsonPointerToken { token } => {
                        assert_eq!(token, "Missing");
                    }
                    other => panic!("unexpected reference source: {other}"),
                }
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
