use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::error::{ParseError, ValidationError};
use crate::ident::{
    field_ident, function_ident, response_variant_ident, type_ident, unique_ident, variant_ident,
};
use crate::model::{
    Api, Component, ComponentKind, ConstrainedType, EnumVariant, Field, FloatLimit, HttpMethod,
    IntegerLimit, Operation, Parameter, ParameterLocation, PathSegment, RequestBody, ResponseCase,
    TypeRef, Validation, is_array_type,
};

pub(crate) fn parse_api(document: &Value) -> Result<Api, ValidationError> {
    let root = object(document, "OpenAPI document")?;
    let openapi = required_str(root, "openapi", "OpenAPI document")?;
    if !openapi.starts_with("3.0") {
        return Err(ValidationError::UnsupportedOpenApiVersion {
            version: openapi.to_owned(),
        });
    }

    let mut registry = TypeRegistry::default();
    reserve_component_type_names(document, &mut registry)?;
    let components = parse_components(document, &mut registry)?;
    let operations = parse_operations(document, &mut registry)?;

    Ok(Api {
        components,
        constrained_types: registry.generated,
        operations,
    })
}

#[derive(Debug, Default)]
struct TypeRegistry {
    generated: Vec<ConstrainedType>,
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
}

pub(crate) fn parse_document(spec: &str) -> Result<Value, ParseError> {
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(spec).map_err(ParseError::Document)?;
    serde_json::to_value(yaml).map_err(ParseError::NormalizeDocument)
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

    let mut used = BTreeSet::new();
    let mut variants = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value.as_str() else {
            return Err(ValidationError::NonStringEnumValue {
                context: context.to_owned(),
            });
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
    if schema.contains_key("enum") {
        return Ok(TypeRef::String);
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
        TypeRef::Bool | TypeRef::Named(_) | TypeRef::Constrained { .. } | TypeRef::Nullable(_) => {
            Ok(None)
        }
    }
}

fn parse_string_validation(
    schema: &Map<String, Value>,
    context: &str,
) -> Result<Option<Validation>, ValidationError> {
    if schema.contains_key("pattern") {
        return Err(ValidationError::UnsupportedPattern {
            context: context.to_owned(),
        });
    }

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

    let required = match location {
        ParameterLocation::Path => {
            if parameter.get("required").and_then(Value::as_bool) != Some(true) {
                return Err(ValidationError::PathParameterNotRequired {
                    wire_name: wire_name.clone(),
                });
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
