#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::fmt::Write as _;

use heck::{ToSnakeCase, ToUpperCamelCase};
use serde_json::{Map, Value};

pub fn generate(spec: &str) -> Result<String, Error> {
    let document = parse_document(spec)?;
    let api = Api::from_document(&document)?;
    render_api(&api)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    message: String,
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug)]
struct Api {
    components: Vec<Component>,
    constrained_types: Vec<ConstrainedType>,
    operations: Vec<Operation>,
}

#[derive(Debug)]
struct Component {
    rust_name: String,
    kind: ComponentKind,
}

#[derive(Debug)]
enum ComponentKind {
    Struct(Vec<Field>),
    Enum(Vec<EnumVariant>),
    Alias(TypeRef),
    Nutype(ConstrainedType),
}

#[derive(Debug, Clone)]
struct ConstrainedType {
    rust_name: String,
    inner: TypeRef,
    validation: Validation,
}

#[derive(Debug, Clone, PartialEq)]
enum Validation {
    String {
        min_length: Option<u64>,
        max_length: Option<u64>,
    },
    Integer {
        minimum: Option<IntegerLimit>,
        maximum: Option<IntegerLimit>,
    },
    Number {
        minimum: Option<FloatLimit>,
        maximum: Option<FloatLimit>,
    },
    Array {
        min_items: Option<u64>,
        max_items: Option<u64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IntegerLimit {
    value: i128,
    exclusive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FloatLimit {
    value: f64,
    exclusive: bool,
}

#[derive(Debug, Clone)]
struct Field {
    wire_name: String,
    rust_name: String,
    ty: TypeRef,
    required: bool,
}

#[derive(Debug)]
struct EnumVariant {
    wire_name: String,
    rust_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeRef {
    String,
    I32,
    I64,
    F32,
    F64,
    Bool,
    Array(Box<TypeRef>),
    Named(String),
    Constrained {
        rust_name: String,
        inner: Box<TypeRef>,
    },
    Nullable(Box<TypeRef>),
}

#[derive(Debug)]
struct Operation {
    fn_name: String,
    input_name: String,
    response_name: String,
    method: HttpMethod,
    path: String,
    path_segments: Vec<PathSegment>,
    parameters: Vec<Parameter>,
    request_body: Option<RequestBody>,
    responses: Vec<ResponseCase>,
}

#[derive(Debug, Default)]
struct TypeRegistry {
    generated: Vec<ConstrainedType>,
    used_names: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HttpMethod {
    Delete,
    Get,
    Head,
    Options,
    Patch,
    Post,
    Put,
    Trace,
}

#[derive(Debug)]
enum PathSegment {
    Literal(String),
    Parameter(String),
}

#[derive(Debug, Clone)]
struct Parameter {
    location: ParameterLocation,
    wire_name: String,
    rust_name: String,
    ty: TypeRef,
    required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParameterLocation {
    Path,
    Query,
}

#[derive(Debug)]
struct RequestBody {
    field_name: String,
    content_type: String,
    ty: TypeRef,
    required: bool,
}

#[derive(Debug)]
struct ResponseCase {
    status: u16,
    variant_name: String,
    body: Option<TypeRef>,
}

impl Api {
    fn from_document(document: &Value) -> Result<Self, Error> {
        let root = object(document, "OpenAPI document")?;
        let openapi = required_str(root, "openapi", "OpenAPI document")?;
        if !openapi.starts_with("3.0") {
            return Err(Error::new(format!(
                "unsupported OpenAPI version `{openapi}`; Satay MVP supports OpenAPI 3.0"
            )));
        }

        let mut registry = TypeRegistry::default();
        reserve_component_type_names(document, &mut registry)?;
        let components = parse_components(document, &mut registry)?;
        let operations = parse_operations(document, &mut registry)?;

        Ok(Self {
            components,
            constrained_types: registry.generated,
            operations,
        })
    }
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
}

impl TypeRef {
    fn rust_type(&self) -> String {
        match self {
            Self::String => "String".to_owned(),
            Self::I32 => "i32".to_owned(),
            Self::I64 => "i64".to_owned(),
            Self::F32 => "f32".to_owned(),
            Self::F64 => "f64".to_owned(),
            Self::Bool => "bool".to_owned(),
            Self::Array(item) => format!("Vec<{}>", item.rust_type()),
            Self::Named(name) => name.clone(),
            Self::Constrained { rust_name, .. } => rust_name.clone(),
            Self::Nullable(inner) => format!("Option<{}>", inner.rust_type()),
        }
    }

    fn rust_field_type(&self, required: bool) -> String {
        if required || self.is_nullable() {
            self.rust_type()
        } else {
            format!("Option<{}>", self.rust_type())
        }
    }

    fn is_nullable(&self) -> bool {
        matches!(self, Self::Nullable(_))
    }

    fn non_nullable(&self) -> &TypeRef {
        match self {
            Self::Nullable(inner) => inner.non_nullable(),
            other => other,
        }
    }
}

fn is_array_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Array(_) => true,
        TypeRef::Constrained { inner, .. } => is_array_type(inner.non_nullable()),
        TypeRef::Nullable(inner) => is_array_type(inner.non_nullable()),
        _ => false,
    }
}

impl HttpMethod {
    fn from_key(key: &str) -> Option<Self> {
        match key {
            "delete" => Some(Self::Delete),
            "get" => Some(Self::Get),
            "head" => Some(Self::Head),
            "options" => Some(Self::Options),
            "patch" => Some(Self::Patch),
            "post" => Some(Self::Post),
            "put" => Some(Self::Put),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }

    fn rust_const(self) -> &'static str {
        match self {
            Self::Delete => "DELETE",
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Patch => "PATCH",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Trace => "TRACE",
        }
    }

    fn operation_prefix(self) -> &'static str {
        match self {
            Self::Delete => "delete",
            Self::Get => "get",
            Self::Head => "head",
            Self::Options => "options",
            Self::Patch => "patch",
            Self::Post => "post",
            Self::Put => "put",
            Self::Trace => "trace",
        }
    }
}

fn parse_document(spec: &str) -> Result<Value, Error> {
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(spec)
        .map_err(|err| Error::new(format!("failed to parse OpenAPI YAML/JSON: {err}")))?;
    serde_json::to_value(yaml)
        .map_err(|err| Error::new(format!("failed to normalize OpenAPI document: {err}")))
}

fn reserve_component_type_names(
    document: &Value,
    registry: &mut TypeRegistry,
) -> Result<(), Error> {
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
) -> Result<Vec<Component>, Error> {
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
) -> Result<ComponentKind, Error> {
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
        Some(kind) => Err(Error::new(format!(
            "unsupported type `{kind}` in schema `{schema_name}`"
        ))),
        None => Err(Error::new(format!(
            "schema `{schema_name}` must declare `type`, `$ref`, `enum`, or `properties`"
        ))),
    }
}

fn parse_component_alias_or_nutype(
    schema_name: &str,
    schema: &Map<String, Value>,
    registry: &mut TypeRegistry,
) -> Result<ComponentKind, Error> {
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
) -> Result<Vec<EnumVariant>, Error> {
    if let Some(kind) = schema.get("type").and_then(Value::as_str)
        && kind != "string"
    {
        return Err(Error::new(format!(
            "{context} uses enum type `{kind}`; only string enums are supported"
        )));
    }

    let values = schema
        .get("enum")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new(format!("{context} has a non-array enum")))?;
    if values.is_empty() {
        return Err(Error::new(format!("{context} has an empty enum")));
    }

    let mut used = BTreeSet::new();
    let mut variants = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value.as_str() else {
            return Err(Error::new(format!(
                "{context} contains a non-string enum value; only string enums are supported"
            )));
        };
        let rust_name = unique_ident(variant_ident(value), &mut used);
        variants.push(EnumVariant {
            wire_name: value.to_owned(),
            rust_name,
        });
    }

    Ok(variants)
}

fn parse_struct_fields(
    schema_name: &str,
    schema: &Map<String, Value>,
    registry: &mut TypeRegistry,
) -> Result<Vec<Field>, Error> {
    let context = format!("schema `{schema_name}`");
    let required = parse_required_set(schema, &context)?;
    reject_keyword(schema, "minProperties", &context)?;
    reject_keyword(schema, "maxProperties", &context)?;
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            Error::new(format!(
                "object schema `{schema_name}` must declare `properties`"
            ))
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
        fields.push(Field {
            wire_name: wire_name.clone(),
            rust_name,
            ty,
            required: required.contains(wire_name),
        });
    }

    Ok(fields)
}

fn parse_required_set(
    schema: &Map<String, Value>,
    context: &str,
) -> Result<BTreeSet<String>, Error> {
    let Some(required) = schema.get("required") else {
        return Ok(BTreeSet::new());
    };
    let required = required
        .as_array()
        .ok_or_else(|| Error::new(format!("{context} has a non-array `required` field")))?;
    let mut set = BTreeSet::new();
    for value in required {
        let Some(name) = value.as_str() else {
            return Err(Error::new(format!(
                "{context} has a non-string required field name"
            )));
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
) -> Result<TypeRef, Error> {
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
) -> Result<TypeRef, Error> {
    if schema.contains_key("enum") {
        return Ok(TypeRef::String);
    }

    match schema.get("type").and_then(Value::as_str) {
        Some("string") => Ok(TypeRef::String),
        Some("integer") => match schema.get("format").and_then(Value::as_str) {
            Some("int32") => Ok(TypeRef::I32),
            Some("int64") | None => Ok(TypeRef::I64),
            Some(format) => Err(Error::new(format!(
                "{context} uses unsupported integer format `{format}`"
            ))),
        },
        Some("number") => match schema.get("format").and_then(Value::as_str) {
            Some("float") => Ok(TypeRef::F32),
            Some("double") | None => Ok(TypeRef::F64),
            Some(format) => Err(Error::new(format!(
                "{context} uses unsupported number format `{format}`"
            ))),
        },
        Some("boolean") => Ok(TypeRef::Bool),
        Some("array") => {
            let items = schema.get("items").ok_or_else(|| {
                Error::new(format!("{context} array schema must declare `items`"))
            })?;
            let item_name_hint = type_name_hint.map(|name| format!("{name} item"));
            Ok(TypeRef::Array(Box::new(parse_type_ref(
                items,
                &format!("{context} items"),
                registry,
                item_name_hint.as_deref(),
            )?)))
        }
        Some("object") | None if schema.contains_key("properties") => Err(Error::new(format!(
            "{context} is an inline object schema; move it to components/schemas and use `$ref`"
        ))),
        Some("object") => Err(Error::new(format!(
            "{context} is an object without properties; map/object schemas are not supported yet"
        ))),
        Some(kind) => Err(Error::new(format!(
            "{context} uses unsupported schema type `{kind}`"
        ))),
        None => Err(Error::new(format!(
            "{context} must declare `type`, `$ref`, or `enum`"
        ))),
    }
}

fn parse_validation(
    schema: &Map<String, Value>,
    base: &TypeRef,
    context: &str,
) -> Result<Option<Validation>, Error> {
    match base {
        TypeRef::String => parse_string_validation(schema, context),
        TypeRef::I32 | TypeRef::I64 => parse_integer_validation(schema, base, context),
        TypeRef::F32 | TypeRef::F64 => parse_number_validation(schema, base, context),
        TypeRef::Array(_) => parse_array_validation(schema, context),
        TypeRef::Bool | TypeRef::Named(_) | TypeRef::Constrained { .. } | TypeRef::Nullable(_) => {
            Ok(None)
        }
    }
}

fn parse_string_validation(
    schema: &Map<String, Value>,
    context: &str,
) -> Result<Option<Validation>, Error> {
    if schema.contains_key("pattern") {
        return Err(Error::new(format!(
            "{context} uses `pattern`; OpenAPI patterns use ECMA regex syntax and are not safely supported yet"
        )));
    }

    let min_length = optional_u64_keyword(schema, "minLength", context)?;
    let max_length = optional_u64_keyword(schema, "maxLength", context)?;
    if let (Some(min_length), Some(max_length)) = (min_length, max_length)
        && min_length > max_length
    {
        return Err(Error::new(format!(
            "{context} has minLength {min_length} greater than maxLength {max_length}"
        )));
    }

    if min_length.is_some() || max_length.is_some() {
        Ok(Some(Validation::String {
            min_length,
            max_length,
        }))
    } else {
        Ok(None)
    }
}

fn parse_integer_validation(
    schema: &Map<String, Value>,
    base: &TypeRef,
    context: &str,
) -> Result<Option<Validation>, Error> {
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
) -> Result<Option<Validation>, Error> {
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
) -> Result<Option<Validation>, Error> {
    if schema
        .get("uniqueItems")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(Error::new(format!(
            "{context} uses `uniqueItems`; generated Vec-backed types cannot enforce uniqueness yet"
        )));
    }

    let min_items = optional_u64_keyword(schema, "minItems", context)?;
    let max_items = optional_u64_keyword(schema, "maxItems", context)?;
    if let (Some(min_items), Some(max_items)) = (min_items, max_items)
        && min_items > max_items
    {
        return Err(Error::new(format!(
            "{context} has minItems {min_items} greater than maxItems {max_items}"
        )));
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

fn reject_keyword(schema: &Map<String, Value>, keyword: &str, context: &str) -> Result<(), Error> {
    if schema.contains_key(keyword) {
        return Err(Error::new(format!(
            "{context} uses `{keyword}`, which is not safely supported yet"
        )));
    }
    Ok(())
}

fn optional_u64_keyword(
    schema: &Map<String, Value>,
    keyword: &str,
    context: &str,
) -> Result<Option<u64>, Error> {
    let Some(value) = schema.get(keyword) else {
        return Ok(None);
    };
    match value.as_u64() {
        Some(value) => Ok(Some(value)),
        None => Err(Error::new(format!(
            "{context}.{keyword} must be a non-negative integer"
        ))),
    }
}

fn optional_bool_keyword(
    schema: &Map<String, Value>,
    keyword: &str,
    context: &str,
) -> Result<Option<bool>, Error> {
    let Some(value) = schema.get(keyword) else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| Error::new(format!("{context}.{keyword} must be a boolean")))
}

fn optional_integer_limit(
    schema: &Map<String, Value>,
    keyword: &str,
    exclusive_keyword: &str,
    context: &str,
) -> Result<Option<IntegerLimit>, Error> {
    let exclusive = optional_bool_keyword(schema, exclusive_keyword, context)?;
    let Some(value) = schema.get(keyword) else {
        if exclusive.is_some() {
            return Err(Error::new(format!(
                "{context}.{exclusive_keyword} requires `{keyword}`"
            )));
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
    keyword: &str,
    exclusive_keyword: &str,
    context: &str,
) -> Result<Option<FloatLimit>, Error> {
    let exclusive = optional_bool_keyword(schema, exclusive_keyword, context)?;
    let Some(value) = schema.get(keyword) else {
        if exclusive.is_some() {
            return Err(Error::new(format!(
                "{context}.{exclusive_keyword} requires `{keyword}`"
            )));
        }
        return Ok(None);
    };
    let value = value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| Error::new(format!("{context}.{keyword} must be a finite number")))?;
    Ok(Some(FloatLimit {
        value,
        exclusive: exclusive.unwrap_or(false),
    }))
}

fn json_integer(value: &Value, context: &str) -> Result<i128, Error> {
    let Some(number) = value.as_number() else {
        return Err(Error::new(format!("{context} must be an integer")));
    };
    if let Some(value) = number.as_i64() {
        return Ok(i128::from(value));
    }
    if let Some(value) = number.as_u64() {
        return Ok(i128::from(value));
    }
    let Some(value) = number.as_f64() else {
        return Err(Error::new(format!("{context} must be an integer")));
    };
    if !value.is_finite() || value.fract() != 0.0 {
        return Err(Error::new(format!("{context} must be an integer")));
    }
    Ok(value as i128)
}

fn normalize_integer_limits(
    minimum: Option<IntegerLimit>,
    maximum: Option<IntegerLimit>,
    base: &TypeRef,
    context: &str,
) -> Result<(Option<IntegerLimit>, Option<IntegerLimit>), Error> {
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
        return Err(Error::new(format!(
            "{context} integer bounds do not allow any value"
        )));
    }

    let minimum = minimum.filter(|_| effective_min > type_min);
    let maximum = maximum.filter(|_| effective_max < type_max);
    Ok((minimum, maximum))
}

fn effective_integer_min(limit: IntegerLimit) -> Result<i128, Error> {
    if limit.exclusive {
        limit
            .value
            .checked_add(1)
            .ok_or_else(|| Error::new("exclusive integer minimum overflows"))
    } else {
        Ok(limit.value)
    }
}

fn effective_integer_max(limit: IntegerLimit) -> Result<i128, Error> {
    if limit.exclusive {
        limit
            .value
            .checked_sub(1)
            .ok_or_else(|| Error::new("exclusive integer maximum overflows"))
    } else {
        Ok(limit.value)
    }
}

fn normalize_float_limits(
    minimum: Option<FloatLimit>,
    maximum: Option<FloatLimit>,
    base: &TypeRef,
    context: &str,
) -> Result<(Option<FloatLimit>, Option<FloatLimit>), Error> {
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
        return Err(Error::new(format!(
            "{context} number bounds do not allow any value"
        )));
    }

    let minimum = minimum.filter(|limit| limit.value > type_min);
    let maximum = maximum.filter(|limit| limit.value < type_max);
    Ok((minimum, maximum))
}

fn parse_operations(
    document: &Value,
    registry: &mut TypeRegistry,
) -> Result<Vec<Operation>, Error> {
    let root = object(document, "OpenAPI document")?;
    let paths = root
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::new("OpenAPI document must declare `paths`"))?;

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
) -> Result<Operation, Error> {
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
        operation.get("responses").ok_or_else(|| {
            Error::new(format!("operation `{operation_id}` must declare responses"))
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
) -> Result<Vec<Parameter>, Error> {
    let Some(parameters) = parameters else {
        return Ok(vec![]);
    };
    let parameters = parameters
        .as_array()
        .ok_or_else(|| Error::new(format!("{context} must be an array")))?;

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
) -> Result<Parameter, Error> {
    let parameter = resolve_reference(document, parameter, context)?;
    let parameter = object(parameter, context)?;
    let wire_name = required_str(parameter, "name", context)?.to_owned();
    let location = match required_str(parameter, "in", context)? {
        "path" => ParameterLocation::Path,
        "query" => ParameterLocation::Query,
        other => {
            return Err(Error::new(format!(
                "{context} parameter `{wire_name}` is in `{other}`; only path and query parameters are supported"
            )));
        }
    };

    if parameter.contains_key("content") {
        return Err(Error::new(format!(
            "{context} parameter `{wire_name}` uses `content`; schema parameters are required"
        )));
    }

    let schema = parameter.get("schema").ok_or_else(|| {
        Error::new(format!(
            "{context} parameter `{wire_name}` must declare schema"
        ))
    })?;
    let ty = parse_type_ref(
        schema,
        &format!("parameter `{wire_name}`"),
        registry,
        Some(&format!("{type_prefix} {wire_name} parameter")),
    )?;
    if ty.is_nullable() {
        return Err(Error::new(format!(
            "parameter `{wire_name}` is nullable; nullable parameters are not supported"
        )));
    }
    if location == ParameterLocation::Path && is_array_type(ty.non_nullable()) {
        return Err(Error::new(format!(
            "path parameter `{wire_name}` is an array; array path parameter styles are not supported"
        )));
    }

    let required = match location {
        ParameterLocation::Path => {
            if parameter.get("required").and_then(Value::as_bool) != Some(true) {
                return Err(Error::new(format!(
                    "path parameter `{wire_name}` must set required: true"
                )));
            }
            true
        }
        ParameterLocation::Query => parameter
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
) -> Result<Option<RequestBody>, Error> {
    let Some(request_body) = request_body else {
        return Ok(None);
    };
    let request_body = resolve_reference(document, request_body, context)?;
    let request_body = object(request_body, context)?;
    let content = request_body
        .get("content")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::new(format!("{context} must declare content")))?;
    let (content_type, media_type) = json_media_type(content)
        .ok_or_else(|| Error::new(format!("{context} must declare application/json content")))?;
    let media_type = object(media_type, context)?;
    let schema = media_type.get("schema").ok_or_else(|| {
        Error::new(format!(
            "{context} application/json content must declare schema"
        ))
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
) -> Result<Vec<ResponseCase>, Error> {
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
                return Err(Error::new(format!(
                    "{context} contains a default response body; default response decoding is not supported yet"
                )));
            }
            continue;
        }
        let status_code = status.parse::<u16>().map_err(|_| {
            Error::new(format!("{context} contains invalid status code `{status}`"))
        })?;
        if !(100..=599).contains(&status_code) {
            return Err(Error::new(format!(
                "{context} contains out-of-range status code `{status_code}`"
            )));
        }

        let response = resolve_reference(document, response, &format!("{context} {status}"))?;
        let response = object(response, &format!("{context} {status}"))?;
        let body = match response.get("content").and_then(Value::as_object) {
            Some(content) if content.is_empty() => None,
            Some(content) => {
                let (_, media_type) = json_media_type(content).ok_or_else(|| {
                    Error::new(format!(
                        "{context} {status} response must declare application/json content"
                    ))
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

fn parse_path_segments(path: &str) -> Result<Vec<PathSegment>, Error> {
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
            return Err(Error::new(format!(
                "path `{path}` contains an unclosed parameter"
            )));
        };
        let name = &after_open[..close];
        if name.is_empty() {
            return Err(Error::new(format!(
                "path `{path}` contains an empty parameter"
            )));
        }
        segments.push(PathSegment::Parameter(name.to_owned()));
        rest = &after_open[close + 1..];
    }
}

fn validate_path_parameters(
    path: &str,
    path_segments: &[PathSegment],
    parameters: &[Parameter],
) -> Result<(), Error> {
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
                return Err(Error::new(format!(
                    "path `{path}` uses parameter `{name}` but it is not declared"
                )));
            }
        }
    }

    for name in declared {
        if !placeholders.contains(name) {
            return Err(Error::new(format!(
                "path parameter `{name}` is declared but not used in path `{path}`"
            )));
        }
    }

    Ok(())
}

fn render_api(api: &Api) -> Result<String, Error> {
    let mut raw = String::new();
    for component in &api.components {
        render_component(&mut raw, component);
        raw.push('\n');
    }
    for constrained_type in &api.constrained_types {
        render_constrained_type(&mut raw, constrained_type);
        raw.push('\n');
    }
    for operation in &api.operations {
        render_input(&mut raw, operation);
        raw.push('\n');
        render_response(&mut raw, operation);
        raw.push('\n');
        render_parts_function(&mut raw, operation);
        raw.push('\n');
        render_encode_function(&mut raw, operation);
        raw.push('\n');
        render_decode_function(&mut raw, operation);
        raw.push('\n');
    }

    let syntax = syn::parse_file(&raw).map_err(|err| {
        Error::new(format!(
            "generated invalid Rust source: {err}\n--- generated source ---\n{raw}"
        ))
    })?;

    let mut formatted = String::from("// @generated by satay. Do not edit by hand.\n\n");
    formatted.push_str(&prettyplease::unparse(&syntax));
    Ok(formatted)
}

fn render_component(out: &mut String, component: &Component) {
    match &component.kind {
        ComponentKind::Struct(fields) => render_struct(out, &component.rust_name, fields, true),
        ComponentKind::Enum(variants) => render_enum(out, &component.rust_name, variants),
        ComponentKind::Alias(ty) => {
            let _ = writeln!(
                out,
                "pub type {} = {};",
                component.rust_name,
                ty.rust_type()
            );
        }
        ComponentKind::Nutype(constrained_type) => render_constrained_type(out, constrained_type),
    }
}

fn render_constrained_type(out: &mut String, constrained_type: &ConstrainedType) {
    let _ = writeln!(out, "#[nutype::nutype(");
    render_validation(out, &constrained_type.validation);
    let _ = writeln!(
        out,
        "derive({}),",
        nutype_derives(&constrained_type.validation)
    );
    let _ = writeln!(
        out,
        "cfg_attr(feature = \"serde\", derive(Serialize, Deserialize)),"
    );
    let _ = writeln!(out, ")]");
    let _ = writeln!(
        out,
        "pub struct {}({});",
        constrained_type.rust_name,
        constrained_type.inner.rust_type()
    );
}

fn render_validation(out: &mut String, validation: &Validation) {
    match validation {
        Validation::String {
            min_length,
            max_length,
        } => {
            let mut validators = Vec::new();
            if let Some(min_length) = min_length {
                validators.push(format!("len_char_min = {min_length}"));
            }
            if let Some(max_length) = max_length {
                validators.push(format!("len_char_max = {max_length}"));
            }
            let _ = writeln!(out, "validate({}),", validators.join(", "));
        }
        Validation::Integer { minimum, maximum } => {
            let mut validators = Vec::new();
            if let Some(minimum) = minimum {
                validators.push(integer_limit_validator(
                    "greater",
                    "greater_or_equal",
                    *minimum,
                ));
            }
            if let Some(maximum) = maximum {
                validators.push(integer_limit_validator("less", "less_or_equal", *maximum));
            }
            let _ = writeln!(out, "validate({}),", validators.join(", "));
        }
        Validation::Number { minimum, maximum } => {
            let mut validators = vec!["finite".to_owned()];
            if let Some(minimum) = minimum {
                validators.push(float_limit_validator(
                    "greater",
                    "greater_or_equal",
                    *minimum,
                ));
            }
            if let Some(maximum) = maximum {
                validators.push(float_limit_validator("less", "less_or_equal", *maximum));
            }
            let _ = writeln!(out, "validate({}),", validators.join(", "));
        }
        Validation::Array {
            min_items,
            max_items,
        } => {
            let predicate = array_predicate(*min_items, *max_items);
            let _ = writeln!(out, "validate(predicate = {predicate}),");
        }
    }
}

fn nutype_derives(validation: &Validation) -> &'static str {
    match validation {
        Validation::String { .. } => {
            "Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display"
        }
        Validation::Integer { .. } => {
            "Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, AsRef, Deref, TryFrom, Into, Display"
        }
        Validation::Number { .. } => {
            "Debug, Clone, Copy, PartialEq, PartialOrd, AsRef, Deref, TryFrom, Into, Display"
        }
        Validation::Array { .. } => "Debug, Clone, PartialEq, AsRef, Deref, TryFrom, Into",
    }
}

fn integer_limit_validator(
    exclusive_name: &str,
    inclusive_name: &str,
    limit: IntegerLimit,
) -> String {
    if limit.exclusive {
        format!("{exclusive_name} = {}", limit.value)
    } else {
        format!("{inclusive_name} = {}", limit.value)
    }
}

fn float_limit_validator(exclusive_name: &str, inclusive_name: &str, limit: FloatLimit) -> String {
    let value = rust_float(limit.value);
    if limit.exclusive {
        format!("{exclusive_name} = {value}")
    } else {
        format!("{inclusive_name} = {value}")
    }
}

fn array_predicate(min_items: Option<u64>, max_items: Option<u64>) -> String {
    match (min_items, max_items) {
        (Some(min_items), Some(max_items)) => {
            format!("|items| items.len() >= {min_items} && items.len() <= {max_items}")
        }
        (Some(min_items), None) => format!("|items| items.len() >= {min_items}"),
        (None, Some(max_items)) => format!("|items| items.len() <= {max_items}"),
        (None, None) => unreachable!("array validation requires at least one bound"),
    }
}

fn rust_float(value: f64) -> String {
    let rendered = value.to_string();
    if rendered.contains('.') || rendered.contains('e') || rendered.contains('E') {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

fn render_struct(out: &mut String, name: &str, fields: &[Field], serde: bool) {
    let _ = writeln!(out, "#[derive(Debug, Clone, PartialEq)]");
    if serde {
        let _ = writeln!(
            out,
            "#[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]"
        );
    }
    let _ = writeln!(out, "pub struct {name} {{");
    for field in fields {
        if serde {
            let mut serde_attrs = Vec::new();
            if field.rust_name != field.wire_name {
                serde_attrs.push(format!("rename = {}", rust_string(&field.wire_name)));
            }
            if !field.required {
                serde_attrs.push("default".to_owned());
                serde_attrs.push("skip_serializing_if = \"Option::is_none\"".to_owned());
            }
            if !serde_attrs.is_empty() {
                let _ = writeln!(
                    out,
                    "#[cfg_attr(feature = \"serde\", serde({}))]",
                    serde_attrs.join(", ")
                );
            }
        }
        let _ = writeln!(
            out,
            "pub {}: {},",
            field.rust_name,
            field.ty.rust_field_type(field.required)
        );
    }
    let _ = writeln!(out, "}}");
}

fn render_enum(out: &mut String, name: &str, variants: &[EnumVariant]) {
    let _ = writeln!(out, "#[derive(Debug, Clone, PartialEq, Eq)]");
    let _ = writeln!(
        out,
        "#[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]"
    );
    let _ = writeln!(out, "pub enum {name} {{");
    for variant in variants {
        if variant.rust_name != variant.wire_name {
            let _ = writeln!(
                out,
                "#[cfg_attr(feature = \"serde\", serde(rename = {}))]",
                rust_string(&variant.wire_name)
            );
        }
        let _ = writeln!(out, "{},", variant.rust_name);
    }
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "impl {name} {{");
    let _ = writeln!(out, "pub const fn as_str(&self) -> &'static str {{");
    let _ = writeln!(out, "match self {{");
    for variant in variants {
        let _ = writeln!(
            out,
            "Self::{} => {},",
            variant.rust_name,
            rust_string(&variant.wire_name)
        );
    }
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "impl AsRef<str> for {name} {{");
    let _ = writeln!(out, "fn as_ref(&self) -> &str {{ self.as_str() }}");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "impl std::fmt::Display for {name} {{");
    let _ = writeln!(
        out,
        "fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{ f.write_str(self.as_str()) }}"
    );
    let _ = writeln!(out, "}}");
}

fn render_input(out: &mut String, operation: &Operation) {
    let mut fields = operation.parameters.iter().map(|parameter| Field {
        wire_name: parameter.wire_name.clone(),
        rust_name: parameter.rust_name.clone(),
        ty: parameter.ty.clone(),
        required: parameter.required,
    });
    let mut input_fields = Vec::with_capacity(
        operation.parameters.len() + usize::from(operation.request_body.is_some()),
    );
    input_fields.extend(fields.by_ref());
    if let Some(body) = &operation.request_body {
        input_fields.push(Field {
            wire_name: body.field_name.clone(),
            rust_name: body.field_name.clone(),
            ty: body.ty.clone(),
            required: body.required,
        });
    }

    render_struct(out, &operation.input_name, &input_fields, false);
}

fn render_response(out: &mut String, operation: &Operation) {
    let _ = writeln!(out, "#[derive(Debug, Clone, PartialEq)]");
    let _ = writeln!(out, "pub enum {} {{", operation.response_name);
    for response in &operation.responses {
        match &response.body {
            Some(body) => {
                let _ = writeln!(out, "{}({}),", response.variant_name, body.rust_type());
            }
            None => {
                let _ = writeln!(out, "{},", response.variant_name);
            }
        }
    }
    let _ = writeln!(out, "UnexpectedStatus(http::StatusCode, Vec<u8>),");
    let _ = writeln!(out, "}}");
}

fn render_parts_function(out: &mut String, operation: &Operation) {
    let body_type = operation.request_body.as_ref().map_or_else(
        || "()".to_owned(),
        |body| body.ty.rust_field_type(body.required),
    );
    let parts_fn = format!("{}_parts", operation.fn_name);
    let input_name = if operation_uses_input(operation) {
        "input"
    } else {
        "_input"
    };

    let _ = writeln!(
        out,
        "pub fn {parts_fn}({input_name}: {}) -> Result<satay_runtime::RequestParts<{body_type}>, satay_runtime::Error> {{",
        operation.input_name
    );
    let _ = writeln!(
        out,
        "let mut uri = String::with_capacity({});",
        operation.path.len()
    );
    render_path(out, operation);
    render_query(out, operation);
    match &operation.request_body {
        Some(body) => {
            let _ = writeln!(out, "let mut headers = http::HeaderMap::new();");
            if body.required {
                let _ = writeln!(
                    out,
                    "headers.insert(http::header::CONTENT_TYPE, http::HeaderValue::from_static({}));",
                    rust_string(&body.content_type)
                );
            } else {
                let _ = writeln!(out, "if input.{}.is_some() {{", body.field_name);
                let _ = writeln!(
                    out,
                    "headers.insert(http::header::CONTENT_TYPE, http::HeaderValue::from_static({}));",
                    rust_string(&body.content_type)
                );
                let _ = writeln!(out, "}}");
            }
        }
        None => {
            let _ = writeln!(out, "let headers = http::HeaderMap::new();");
        }
    }
    let _ = writeln!(out, "Ok(satay_runtime::RequestParts {{");
    let _ = writeln!(
        out,
        "method: http::Method::{},",
        operation.method.rust_const()
    );
    let _ = writeln!(out, "uri,");
    let _ = writeln!(out, "headers,");
    match &operation.request_body {
        Some(body) => {
            let _ = writeln!(out, "body: input.{},", body.field_name);
        }
        None => {
            let _ = writeln!(out, "body: (),");
        }
    }
    let _ = writeln!(out, "}})");
    let _ = writeln!(out, "}}");
}

fn render_path(out: &mut String, operation: &Operation) {
    for segment in &operation.path_segments {
        match segment {
            PathSegment::Literal(literal) if !literal.is_empty() => {
                let _ = writeln!(out, "uri.push_str({});", rust_string(literal));
            }
            PathSegment::Literal(_) => {}
            PathSegment::Parameter(name) => {
                let parameter = operation
                    .parameters
                    .iter()
                    .find(|parameter| {
                        parameter.location == ParameterLocation::Path
                            && parameter.wire_name == *name
                    })
                    .expect("path parameters validated before render");
                let expr = value_expr(&format!("input.{}", parameter.rust_name), &parameter.ty);
                let _ = writeln!(out, "satay_runtime::append_path_segment(&mut uri, {expr});");
            }
        }
    }
}

fn render_query(out: &mut String, operation: &Operation) {
    let query_parameters = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Query)
        .collect::<Vec<_>>();
    if query_parameters.is_empty() {
        return;
    }

    let _ = writeln!(out, "let mut first_query = true;");
    for parameter in query_parameters {
        render_query_parameter(out, parameter);
    }
}

fn render_query_parameter(out: &mut String, parameter: &Parameter) {
    if let Some(item) = array_item_type(parameter.ty.non_nullable()) {
        if parameter.required {
            let values =
                array_values_expr(&format!("input.{}", parameter.rust_name), &parameter.ty);
            let _ = writeln!(out, "for value in {values} {{");
            let expr = value_expr("value", item);
            let _ = writeln!(
                out,
                "satay_runtime::append_query_pair(&mut uri, &mut first_query, {}, {expr});",
                rust_string(&parameter.wire_name)
            );
            let _ = writeln!(out, "}}");
        } else {
            let _ = writeln!(
                out,
                "if let Some(values) = &input.{} {{",
                parameter.rust_name
            );
            let values = array_values_expr("values", &parameter.ty);
            let _ = writeln!(out, "for value in {values} {{");
            let expr = value_expr("value", item);
            let _ = writeln!(
                out,
                "satay_runtime::append_query_pair(&mut uri, &mut first_query, {}, {expr});",
                rust_string(&parameter.wire_name)
            );
            let _ = writeln!(out, "}}");
            let _ = writeln!(out, "}}");
        }
        return;
    }

    if parameter.required {
        let expr = value_expr(&format!("input.{}", parameter.rust_name), &parameter.ty);
        let _ = writeln!(
            out,
            "satay_runtime::append_query_pair(&mut uri, &mut first_query, {}, {expr});",
            rust_string(&parameter.wire_name)
        );
    } else {
        let _ = writeln!(
            out,
            "if let Some(value) = &input.{} {{",
            parameter.rust_name
        );
        let expr = value_expr("value", &parameter.ty);
        let _ = writeln!(
            out,
            "satay_runtime::append_query_pair(&mut uri, &mut first_query, {}, {expr});",
            rust_string(&parameter.wire_name)
        );
        let _ = writeln!(out, "}}");
    }
}

fn array_item_type(ty: &TypeRef) -> Option<&TypeRef> {
    match ty {
        TypeRef::Array(item) => Some(item),
        TypeRef::Constrained { inner, .. } => array_item_type(inner.non_nullable()),
        _ => None,
    }
}

fn array_values_expr(base: &str, ty: &TypeRef) -> String {
    match ty.non_nullable() {
        TypeRef::Array(_) => format!("&{base}"),
        TypeRef::Constrained { inner, .. } if is_array_type(inner.non_nullable()) => {
            format!("{base}.as_ref()")
        }
        _ => unreachable!("array values are only rendered for array types"),
    }
}

fn render_encode_function(out: &mut String, operation: &Operation) {
    let parts_fn = format!("{}_parts", operation.fn_name);
    let encode_fn = format!("encode_{}", operation.fn_name);
    let _ = writeln!(out, "#[cfg(feature = \"json\")]");
    let _ = writeln!(
        out,
        "pub fn {encode_fn}(input: {}) -> Result<http::Request<Vec<u8>>, satay_runtime::Error> {{",
        operation.input_name
    );
    let _ = writeln!(out, "let parts = {parts_fn}(input)?;");
    match &operation.request_body {
        Some(body) if body.required => {
            let _ = writeln!(out, "satay_runtime::into_json_request(parts)");
        }
        Some(_) => {
            let _ = writeln!(out, "satay_runtime::into_optional_json_request(parts)");
        }
        None => {
            let _ = writeln!(out, "satay_runtime::into_empty_request(parts)");
        }
    }
    let _ = writeln!(out, "}}");
}

fn render_decode_function(out: &mut String, operation: &Operation) {
    let decode_fn = format!("decode_{}_response", operation.fn_name);
    let _ = writeln!(out, "#[cfg(feature = \"json\")]");
    let _ = writeln!(
        out,
        "pub fn {decode_fn}(response: http::Response<Vec<u8>>) -> Result<{}, satay_runtime::Error> {{",
        operation.response_name
    );
    let _ = writeln!(out, "let status = response.status();");
    let _ = writeln!(out, "match status.as_u16() {{");
    for response in &operation.responses {
        let _ = writeln!(out, "{} => {{", response.status);
        match &response.body {
            Some(body) => {
                let _ = writeln!(out, "let body = response.into_body();");
                let _ = writeln!(
                    out,
                    "let value = satay_runtime::from_json_slice::<{}>(&body)?;",
                    body.rust_type()
                );
                let _ = writeln!(
                    out,
                    "Ok({}::{}(value))",
                    operation.response_name, response.variant_name
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "Ok({}::{})",
                    operation.response_name, response.variant_name
                );
            }
        }
        let _ = writeln!(out, "}}");
    }
    let _ = writeln!(out, "_ => {{");
    let _ = writeln!(out, "let body = response.into_body();");
    let _ = writeln!(
        out,
        "Ok({}::UnexpectedStatus(status, body))",
        operation.response_name
    );
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "}}");
}

fn operation_uses_input(operation: &Operation) -> bool {
    !operation.parameters.is_empty() || operation.request_body.is_some()
}

fn value_expr(base: &str, ty: &TypeRef) -> String {
    match ty.non_nullable() {
        TypeRef::String => format!("{base}.as_str()"),
        TypeRef::Named(_) => format!("{base}.as_ref()"),
        TypeRef::Constrained { inner, .. } => constrained_value_expr(base, inner.non_nullable()),
        TypeRef::I32 | TypeRef::I64 | TypeRef::F32 | TypeRef::F64 | TypeRef::Bool => {
            format!("&{base}.to_string()")
        }
        TypeRef::Array(_) | TypeRef::Nullable(_) => unreachable!("arrays are handled by caller"),
    }
}

fn constrained_value_expr(base: &str, inner: &TypeRef) -> String {
    match inner {
        TypeRef::String | TypeRef::Named(_) => format!("{base}.as_ref()"),
        TypeRef::I32 | TypeRef::I64 | TypeRef::F32 | TypeRef::F64 | TypeRef::Bool => {
            format!("&{base}.to_string()")
        }
        TypeRef::Array(_) | TypeRef::Constrained { .. } | TypeRef::Nullable(_) => {
            unreachable!("arrays are handled by caller")
        }
    }
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

fn schema_ref_type_name(reference: &str) -> Result<String, Error> {
    let name = local_ref_name(reference, "schemas")?;
    Ok(type_ident(&name))
}

fn resolve_reference<'a>(
    document: &'a Value,
    value: &'a Value,
    context: &str,
) -> Result<&'a Value, Error> {
    let Some(reference) = value
        .as_object()
        .and_then(|object| object.get("$ref"))
        .and_then(Value::as_str)
    else {
        return Ok(value);
    };

    resolve_json_pointer(document, reference).map_err(|err| {
        Error::new(format!(
            "failed to resolve reference `{reference}` in {context}: {err}"
        ))
    })
}

fn resolve_json_pointer<'a>(document: &'a Value, reference: &str) -> Result<&'a Value, Error> {
    let Some(pointer) = reference.strip_prefix('#') else {
        return Err(Error::new("only local references are supported"));
    };
    if pointer.is_empty() {
        return Ok(document);
    }
    if !pointer.starts_with('/') {
        return Err(Error::new("local reference must be a JSON pointer"));
    }

    let mut current = document;
    for token in pointer[1..].split('/') {
        let token = json_pointer_unescape(token);
        current = current
            .as_object()
            .and_then(|object| object.get(&token))
            .ok_or_else(|| Error::new(format!("missing `{token}`")))?;
    }
    Ok(current)
}

fn local_ref_name(reference: &str, section: &str) -> Result<String, Error> {
    let prefix = format!("#/components/{section}/");
    let Some(name) = reference.strip_prefix(&prefix) else {
        return Err(Error::new(format!(
            "reference `{reference}` must point to #/components/{section}/..."
        )));
    };
    Ok(json_pointer_unescape(name))
}

fn json_pointer_unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>, Error> {
    value
        .as_object()
        .ok_or_else(|| Error::new(format!("{context} must be an object")))
}

fn optional_object<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<Option<&'a Map<String, Value>>, Error> {
    match object.get(field) {
        Some(value) => value
            .as_object()
            .map(Some)
            .ok_or_else(|| Error::new(format!("{context}.{field} must be an object"))),
        None => Ok(None),
    }
}

fn required_str<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<&'a str, Error> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new(format!("{context} must declare string field `{field}`")))
}

fn reject_composition(schema: &Map<String, Value>, context: &str) -> Result<(), Error> {
    for keyword in ["oneOf", "anyOf", "allOf"] {
        if schema.contains_key(keyword) {
            return Err(Error::new(format!(
                "{context} uses `{keyword}`, which is not in the MVP scope"
            )));
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

fn type_ident(value: &str) -> String {
    let ident = value.to_upper_camel_case();
    sanitize_ident(&ident, "GeneratedType", IdentKind::Type)
}

fn variant_ident(value: &str) -> String {
    let ident = value.to_upper_camel_case();
    sanitize_ident(&ident, "Value", IdentKind::Type)
}

fn response_variant_ident(status: u16) -> String {
    match status {
        200 => "Ok".to_owned(),
        201 => "Created".to_owned(),
        202 => "Accepted".to_owned(),
        204 => "NoContent".to_owned(),
        400 => "BadRequest".to_owned(),
        401 => "Unauthorized".to_owned(),
        403 => "Forbidden".to_owned(),
        404 => "NotFound".to_owned(),
        409 => "Conflict".to_owned(),
        422 => "UnprocessableEntity".to_owned(),
        500 => "InternalServerError".to_owned(),
        _ => format!("Status{status}"),
    }
}

fn field_ident(value: &str) -> String {
    let ident = value.to_snake_case();
    sanitize_ident(&ident, "field", IdentKind::Value)
}

fn function_ident(value: &str) -> String {
    let ident = value.to_snake_case();
    sanitize_ident(&ident, "operation", IdentKind::Value)
}

#[derive(Debug, Clone, Copy)]
enum IdentKind {
    Type,
    Value,
}

fn sanitize_ident(value: &str, fallback: &str, kind: IdentKind) -> String {
    let mut ident = String::with_capacity(value.len().max(fallback.len()));
    for ch in value.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            ident.push(ch);
        } else if !ident.ends_with('_') {
            ident.push('_');
        }
    }

    while ident.starts_with('_') {
        ident.remove(0);
    }
    while ident.ends_with('_') {
        ident.pop();
    }
    if ident.is_empty() {
        ident.push_str(fallback);
    }

    let starts_with_digit = ident
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_digit());
    if starts_with_digit {
        match kind {
            IdentKind::Type => ident.insert_str(0, fallback),
            IdentKind::Value => ident.insert(0, '_'),
        }
    }

    if is_rust_keyword(&ident) {
        ident.push('_');
    }
    ident
}

fn unique_ident(candidate: String, used: &mut BTreeSet<String>) -> String {
    if used.insert(candidate.clone()) {
        return candidate;
    }

    for suffix in 2.. {
        let next = format!("{candidate}_{suffix}");
        if used.insert(next.clone()) {
            return next;
        }
    }
    unreachable!()
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
    )
}

fn rust_string(value: &str) -> String {
    format!("{value:?}")
}
