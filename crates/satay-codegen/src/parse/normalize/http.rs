//! Owned HTTP tree normalization for the OpenAPI-to-IR frontend.
//!
//! The converter mirrors the legacy `validate_operations`/`validate_operation`
//! structure but emits owned [`satay_ir`] records: path-level and
//! operation-local parameter lists stay separate in declaration order, every
//! declared media entry is preserved with its original spelling, responses
//! keep their declared status order, and projection selectors retain the
//! original response envelope next to the projected output. Presence of
//! `servers` and `security` declarations is read from the sparse presence
//! index at each declaration's physical pointer so absent and explicitly
//! empty declarations stay distinguishable.

use std::collections::BTreeSet;

use oas3::Map as OasMap;
use oas3::spec::{
    Flows as OasFlows, LocalComponentRef, ObjectOrReference, Operation as OasOperation,
    Parameter as OasParameter, ParameterIn as OasParameterIn, ParameterStyle as OasParameterStyle,
    PathItem as OasPathItem, RequestBody as OasRequestBody, ResolvableComponent,
    Response as OasResponse, Schema as OasSchema, SchemaType as OasSchemaType,
    SecurityRequirement as OasSecurityRequirement, SecurityScheme as OasSecurityScheme,
    Server as OasServer,
};
use oas3::spec::{MediaType as OasMediaType, ObjectSchema};
use serde_json::Value;

use super::source::{child_pointer, source_ref};
use super::{NormalizeContext, NormalizeError, SchemaPosition, ValidationErrorExt};
use crate::error::ValidationError;
use crate::parse::helpers::{json_media_type, optional_description};
use crate::parse::normalize::source::PresenceIndex;
use crate::parse::reference::{schema_component_ref, schema_type_and_nullable};
use crate::parse::satay::{SatayOperationOptions, SatayOutputOptions, operation_options};
use crate::parse::validate::operation::{path_parameter_names, wildcard_status_class};
use satay_ir::{
    ApiKeyLocation, ArrayConstraints, ArraySchema, HttpApi, HttpMethod, MediaType, OAuthFlow,
    OAuthFlowKind, OAuthScope, Operation, OperationInterpretation, OutputSelector, Parameter,
    ParameterLocation, ParameterStyle, PathItem, RequestBody, Response, ResponseMediaType,
    ResponseProjection, ResponseStatus, SchemaAnnotations, SchemaUse, SecurityRequirement,
    SecurityRequirementScheme, SecurityScheme, SecuritySchemeKind, Server, ServerVariable, Tag,
    TypeExpr,
};

impl NormalizeContext<'_, '_> {
    /// Normalizes the owned HTTP tree of the resolved document.
    ///
    /// # Errors
    ///
    /// Reports a missing `paths` field, skip-selection placement, parameter
    /// and media normalization, projection failures, and metadata mapping
    /// failures as structured [`NormalizeError`] values located at the
    /// offending node.
    #[allow(clippy::too_many_lines)]
    pub(in crate::parse) fn http(&self) -> Result<HttpApi, NormalizeError> {
        let Some(paths) = self.document.spec.paths.as_ref() else {
            return Err(ValidationError::MissingPaths.at(self, ""));
        };

        let mut items = Vec::with_capacity(paths.len());
        for (path, path_item) in paths {
            let use_pointer = child_pointer("/paths", path);
            let context = format!("path item `{path}`");
            let resolved = self
                .document
                .resolve_path_item(path_item, &context)
                .map_err(|error| error.at(self, &use_pointer))?;
            let physical_pointer = self.path_item_pointer(path_item, &use_pointer, &context)?;

            // Only the outer path record names a Reference Object use site.
            // Its children occur at the terminal declaration, not under $ref.
            let mut present = false;
            let mut retained = vec![];

            for (method, wire, operation) in [
                (HttpMethod::Get, "get", resolved.get.as_ref()),
                (HttpMethod::Post, "post", resolved.post.as_ref()),
                (HttpMethod::Put, "put", resolved.put.as_ref()),
                (HttpMethod::Patch, "patch", resolved.patch.as_ref()),
                (HttpMethod::Delete, "delete", resolved.delete.as_ref()),
                (HttpMethod::Head, "head", resolved.head.as_ref()),
                (HttpMethod::Options, "options", resolved.options.as_ref()),
                (HttpMethod::Trace, "trace", resolved.trace.as_ref()),
            ]
            .into_iter()
            .filter_map(|(method, wire, operation)| {
                operation.map(|operation| (method, wire, operation))
            }) {
                present = true;
                let context_id = operation
                    .operation_id
                    .clone()
                    .unwrap_or_else(|| format!("{wire} {path}"));
                let operation_pointer = child_pointer(&physical_pointer, wire);
                let options = operation_options(operation, &format!("operation `{context_id}`"))
                    .map_err(|error| {
                        let extension_pointer = child_pointer(&operation_pointer, "x-satay");
                        let pointer = match &error {
                            ValidationError::InvalidExtension { path, .. }
                                if path == "x-satay.output"
                                    || path.starts_with("x-satay.output.") =>
                            {
                                child_pointer(&extension_pointer, "output")
                            }
                            _ => extension_pointer,
                        };
                        error.at(self, &pointer)
                    })?
                    .unwrap_or_default();
                if !options.skip {
                    retained.push((method, operation, context_id, operation_pointer, options));
                }
            }
            // All-skipped paths bypass even unsupported shared parameters.
            // Paths with no operations still retain their parameters.
            if present && retained.is_empty() {
                continue;
            }

            let mut parameters = Vec::with_capacity(resolved.parameters.len());
            let parameters_pointer = child_pointer(&physical_pointer, "parameters");
            for (index, parameter) in resolved.parameters.iter().enumerate() {
                parameters.push(self.parameter(
                    parameter,
                    &child_pointer(&parameters_pointer, &index.to_string()),
                    &format!("{context} parameters"),
                )?);
            }

            let mut operations = Vec::with_capacity(retained.len());
            for (method, operation, context_id, operation_pointer, options) in retained {
                operations.push(self.operation(
                    method,
                    path,
                    &use_pointer,
                    operation,
                    &context_id,
                    &parameters,
                    &operation_pointer,
                    &options,
                )?);
            }

            items.push(PathItem {
                path: path.to_owned(),
                parameters,
                operations,
                servers: self
                    .presence
                    .has(&physical_pointer, "servers")
                    .then(|| map_servers(&resolved.servers)),
                source: Some(source_ref(self.document_id, &use_pointer)),
            });
        }

        Ok(HttpApi {
            paths: items,
            servers: map_servers(&self.document.spec.servers),
            security_schemes: self.security_schemes()?,
            security: self
                .document
                .spec
                .security
                .iter()
                .map(requirement_alternative)
                .collect(),
            tags: self
                .document
                .spec
                .tags
                .iter()
                .map(|tag| Tag {
                    name: tag.name.clone(),
                    description: optional_description(&tag.description),
                })
                .collect(),
        })
    }

    /// Converts one retained operation with its local parameters and body.
    ///
    /// Every emitted operation has `OperationInterpretation.skip == false`
    /// because skipped operations were removed during selection.
    #[allow(clippy::too_many_arguments)]
    fn operation(
        &self,
        method: HttpMethod,
        path: &str,
        path_pointer: &str,
        operation: &OasOperation,
        context_id: &str,
        path_parameters: &[Parameter],
        pointer: &str,
        options: &SatayOperationOptions,
    ) -> Result<Operation, NormalizeError> {
        let context = format!("operation `{context_id}`");
        let mut parameters = Vec::with_capacity(operation.parameters.len());
        let parameters_pointer = child_pointer(pointer, "parameters");
        for (index, parameter) in operation.parameters.iter().enumerate() {
            parameters.push(self.parameter(
                parameter,
                &child_pointer(&parameters_pointer, &index.to_string()),
                &context,
            )?);
        }

        // Check the effective path-name set without merging the stored lists.
        let declared = path_parameters
            .iter()
            .chain(&parameters)
            .filter(|parameter| parameter.location == ParameterLocation::Path)
            .map(|parameter| parameter.wire_name.as_str())
            .collect::<BTreeSet<_>>();
        let placeholders =
            path_parameter_names(path).map_err(|error| error.at(self, path_pointer))?;
        for name in &placeholders {
            if !declared.contains(name.as_str()) {
                return Err(ValidationError::UndeclaredPathParameter {
                    path: path.to_owned(),
                    name: name.clone(),
                }
                .at(self, path_pointer));
            }
        }
        for name in declared {
            if !placeholders.contains(name) {
                return Err(ValidationError::UnusedPathParameter {
                    path: path.to_owned(),
                    name: name.to_owned(),
                }
                .at(self, path_pointer));
            }
        }

        let request_body = self.request_body(
            operation.request_body.as_ref(),
            &child_pointer(pointer, "requestBody"),
            &format!("{context} requestBody"),
        )?;
        let output_pointer = child_pointer(&child_pointer(pointer, "x-satay"), "output");
        let responses = self.responses(
            operation.responses.as_ref(),
            pointer,
            &format!("{context} responses"),
            context_id,
            options.output.as_ref(),
        )?;
        if options.output.is_some()
            && !responses.iter().any(|response| {
                response
                    .content
                    .iter()
                    .any(|media| media.projection.is_some())
            })
        {
            return Err(ValidationError::SatayOutputRequiresResponseBody {
                operation_id: context_id.to_owned(),
            }
            .at(self, &output_pointer));
        }

        Ok(Operation {
            source_id: operation.operation_id.clone(),
            method,
            description: optional_description(&operation.description),
            tags: operation.tags.clone(),
            parameters,
            request_body,
            responses,
            servers: self
                .presence
                .has(pointer, "servers")
                .then(|| map_servers(&operation.servers)),
            security: self.presence.has(pointer, "security").then(|| {
                operation
                    .security
                    .iter()
                    .map(requirement_alternative)
                    .collect()
            }),
            interpretation: OperationInterpretation {
                skip: false,
                output: options.output.as_ref().map(|output| OutputSelector {
                    unwrap_field: output.unwrap_field.as_str().to_owned(),
                    map_field: output
                        .map_field
                        .as_ref()
                        .map(|field| field.as_str().to_owned()),
                }),
            },
            source: Some(source_ref(self.document_id, pointer)),
        })
    }

    /// Converts one parameter, reference or inline.
    fn parameter(
        &self,
        parameter: &ObjectOrReference<OasParameter>,
        use_pointer: &str,
        context: &str,
    ) -> Result<Parameter, NormalizeError> {
        let resolved = self
            .document
            .resolve(parameter, context)
            .map_err(|error| error.at(self, use_pointer))?;
        let physical_pointer = self.component_pointer(parameter, use_pointer, context)?;
        let wire_name = &resolved.name;

        let location = match resolved.location {
            OasParameterIn::Path => ParameterLocation::Path,
            OasParameterIn::Query => ParameterLocation::Query,
            OasParameterIn::Header => ParameterLocation::Header,
            OasParameterIn::Cookie => {
                return Err(ValidationError::UnsupportedParameterLocation {
                    context: context.to_owned(),
                    wire_name: wire_name.clone(),
                    location: "cookie".to_owned(),
                }
                .at(self, &physical_pointer));
            }
        };
        if resolved.content.is_some() {
            return Err(ValidationError::ContentParameterUnsupported {
                context: context.to_owned(),
                wire_name: wire_name.clone(),
            }
            .at(self, &physical_pointer));
        }
        let Some(schema) = resolved.schema.as_ref() else {
            return Err(ValidationError::MissingParameterSchema {
                context: context.to_owned(),
                wire_name: wire_name.clone(),
            }
            .at(self, &physical_pointer));
        };
        let required = if location == ParameterLocation::Path {
            if resolved.required != Some(true) {
                return Err(ValidationError::PathParameterNotRequired {
                    wire_name: wire_name.clone(),
                }
                .at(self, &physical_pointer));
            }
            true
        } else {
            resolved.required.unwrap_or(false)
        };

        Ok(Parameter {
            wire_name: wire_name.clone(),
            location,
            required,
            description: optional_description(&resolved.description),
            schema: self.schema_use(
                schema,
                &child_pointer(&physical_pointer, "schema"),
                SchemaPosition::Value,
                &format!("{context} `{wire_name}`"),
            )?,
            style: resolved.style.map(style_wire),
            explode: resolved.explode,
            allow_reserved: resolved.allow_reserved,
            allow_empty_value: resolved.allow_empty_value,
            source: Some(source_ref(self.document_id, use_pointer)),
        })
    }

    /// Converts one request body declaration.
    fn request_body(
        &self,
        request_body: Option<&ObjectOrReference<OasRequestBody>>,
        use_pointer: &str,
        context: &str,
    ) -> Result<Option<RequestBody>, NormalizeError> {
        let Some(request_body) = request_body else {
            return Ok(None);
        };
        let resolved = self
            .document
            .resolve(request_body, context)
            .map_err(|error| error.at(self, use_pointer))?;
        let physical_pointer = self.component_pointer(request_body, use_pointer, context)?;
        if resolved.content.is_empty() {
            return Err(ValidationError::MissingContent {
                context: context.to_owned(),
            }
            .at(self, &physical_pointer));
        }
        let Some((selected, selected_media)) = json_media_type(&resolved.content) else {
            return Err(ValidationError::MissingJsonContent {
                context: context.to_owned(),
            }
            .at(self, &physical_pointer));
        };
        let content_pointer = child_pointer(&physical_pointer, "content");
        if selected_media.schema.is_none() {
            return Err(ValidationError::MissingJsonSchema {
                context: context.to_owned(),
            }
            .at(self, &child_pointer(&content_pointer, selected)));
        }

        let mut media_types = Vec::with_capacity(resolved.content.len());
        for (media, entry) in &resolved.content {
            let position = if media == selected {
                SchemaPosition::Value
            } else {
                SchemaPosition::RetainedWire
            };
            media_types.push(self.media_entry(
                media,
                entry,
                &content_pointer,
                position,
                context,
            )?);
        }
        Ok(Some(RequestBody {
            description: optional_description(&resolved.description),
            required: resolved.required.unwrap_or(false),
            content: media_types,
            source: Some(source_ref(self.document_id, use_pointer)),
        }))
    }

    /// Converts one declared media association at its physical source.
    fn media_entry(
        &self,
        media: &str,
        entry: &OasMediaType,
        content_pointer: &str,
        position: SchemaPosition,
        context: &str,
    ) -> Result<MediaType, NormalizeError> {
        let pointer = child_pointer(content_pointer, media);
        let schema = entry
            .schema
            .as_ref()
            .map(|schema| {
                self.schema_use(
                    schema,
                    &child_pointer(&pointer, "schema"),
                    position,
                    context,
                )
            })
            .transpose()?;
        Ok(MediaType {
            media_type: media.to_owned(),
            schema,
            source: Some(source_ref(self.document_id, &pointer)),
        })
    }

    /// Converts the declared responses of one operation in map order.
    ///
    /// The `default` response is retained with empty content, wildcard and
    /// exact status selectors are kept as declared, and the projection is
    /// attached to the selected JSON entry when an output selector exists.
    fn responses(
        &self,
        responses: Option<&OasMap<String, ObjectOrReference<OasResponse>>>,
        operation_pointer: &str,
        context: &str,
        context_id: &str,
        output: Option<&SatayOutputOptions>,
    ) -> Result<Vec<Response>, NormalizeError> {
        let Some(responses) = responses else {
            return Err(ValidationError::MissingOperationResponses {
                operation_id: context_id.to_owned(),
            }
            .at(self, operation_pointer));
        };

        let responses_pointer = child_pointer(operation_pointer, "responses");
        let output_pointer = child_pointer(&child_pointer(operation_pointer, "x-satay"), "output");
        let mut converted = Vec::with_capacity(responses.len());
        for (status, response) in responses {
            let use_pointer = child_pointer(&responses_pointer, status);
            let parsed_status = if status == "default" {
                ResponseStatus::Default
            } else if let Some(class) = wildcard_status_class(status) {
                ResponseStatus::Range(class)
            } else {
                let Ok(code) = status.parse::<u16>() else {
                    return Err(ValidationError::InvalidStatusCode {
                        context: context.to_owned(),
                        status: status.to_owned(),
                    }
                    .at(self, &use_pointer));
                };
                if !(100..=599).contains(&code) {
                    return Err(ValidationError::OutOfRangeStatusCode {
                        context: context.to_owned(),
                        status_code: code,
                    }
                    .at(self, &use_pointer));
                }
                ResponseStatus::Exact(code)
            };
            let resolved = self
                .document
                .resolve(response, &format!("{context} {status}"))
                .map_err(|error| error.at(self, &use_pointer))?;
            let physical_pointer = self.component_pointer(response, &use_pointer, context)?;
            if parsed_status == ResponseStatus::Default && !resolved.content.is_empty() {
                return Err(ValidationError::DefaultResponseBodyUnsupported {
                    context: context.to_owned(),
                }
                .at(self, &physical_pointer));
            }

            let mut media_types = Vec::with_capacity(resolved.content.len());
            if !resolved.content.is_empty() {
                let Some((selected, _)) = json_media_type(&resolved.content) else {
                    return Err(ValidationError::MissingResponseJsonContent {
                        context: context.to_owned(),
                        status: status.to_owned(),
                    }
                    .at(self, &physical_pointer));
                };
                let content_pointer = child_pointer(&physical_pointer, "content");
                let schema_context = format!("{context} {status} schema");
                for (media, entry) in &resolved.content {
                    let projection = match (media == selected, entry.schema.as_ref(), output) {
                        (true, Some(schema), Some(output)) => Some(self.projection(
                            schema,
                            &child_pointer(&child_pointer(&content_pointer, media), "schema"),
                            output,
                            &schema_context,
                            &output_pointer,
                        )?),
                        _ => None,
                    };
                    let position = if media == selected && projection.is_none() {
                        SchemaPosition::Value
                    } else {
                        SchemaPosition::RetainedWire
                    };
                    media_types.push(ResponseMediaType {
                        media: self.media_entry(
                            media,
                            entry,
                            &content_pointer,
                            position,
                            &schema_context,
                        )?,
                        projection,
                    });
                }
            }

            converted.push(Response {
                status: parsed_status,
                description: optional_description(&resolved.description),
                content: media_types,
                source: Some(source_ref(self.document_id, &use_pointer)),
            });
        }
        Ok(converted)
    }

    /// Builds the projected output use for one selected response schema.
    ///
    /// The output retains the declared nullability of the projected source;
    /// optionality of an unrequired envelope field stays in the envelope's
    /// retained `required` list instead of being folded into JSON null.
    #[allow(clippy::too_many_lines)]
    fn projection(
        &self,
        schema: &OasSchema,
        schema_pointer: &str,
        output: &SatayOutputOptions,
        context: &str,
        x_satay_pointer: &str,
    ) -> Result<ResponseProjection, NormalizeError> {
        let envelope = self
            .document
            .resolve_schema(schema, context)
            .map_err(|error| error.at(self, schema_pointer))?;
        let Some(object) = envelope.as_object() else {
            return Err(ValidationError::SatayOutputExpectedObject {
                context: context.to_owned(),
                selector: "unwrap-field",
            }
            .at(self, x_satay_pointer));
        };
        let (schema_type, _) = schema_type_and_nullable(object, context)
            .map_err(|error| error.at(self, x_satay_pointer))?;
        if !matches!(schema_type, Some(OasSchemaType::Object) | None)
            || object.properties.is_empty()
        {
            return Err(ValidationError::SatayOutputExpectedObject {
                context: context.to_owned(),
                selector: "unwrap-field",
            }
            .at(self, x_satay_pointer));
        }

        let unwrap_field = output.unwrap_field.as_str();
        let Some(unwrapped) = object.properties.get(unwrap_field) else {
            return Err(ValidationError::UnknownSatayOutputField {
                context: context.to_owned(),
                selector: "unwrap-field",
                field: unwrap_field.to_owned(),
            }
            .at(self, x_satay_pointer));
        };

        let envelope_pointer = self.schema_chain_pointer(schema, schema_pointer, context)?;
        let unwrapped_pointer = child_pointer(
            &child_pointer(&envelope_pointer, "properties"),
            unwrap_field,
        );

        let Some(map_field) = output.map_field.as_ref() else {
            let output = self.schema_use(
                unwrapped,
                &unwrapped_pointer,
                SchemaPosition::Value,
                context,
            )?;
            return Ok(ResponseProjection {
                selector: OutputSelector {
                    unwrap_field: unwrap_field.to_owned(),
                    map_field: None,
                },
                output,
            });
        };

        let map_field = map_field.as_str();
        let array_schema = self
            .document
            .resolve_schema(unwrapped, context)
            .map_err(|error| error.at(self, x_satay_pointer))?;
        let Some(array) = array_schema.as_object() else {
            return Err(ValidationError::SatayOutputMapRequiresArray {
                context: context.to_owned(),
                field: unwrap_field.to_owned(),
            }
            .at(self, x_satay_pointer));
        };
        let (array_type, array_nullable) = schema_type_and_nullable(array, context)
            .map_err(|error| error.at(self, x_satay_pointer))?;
        if array_type != Some(OasSchemaType::Array) {
            return Err(ValidationError::SatayOutputMapRequiresArray {
                context: context.to_owned(),
                field: unwrap_field.to_owned(),
            }
            .at(self, x_satay_pointer));
        }
        let Some(items) = array.items.as_deref() else {
            return Err(ValidationError::MissingArrayItems {
                context: context.to_owned(),
            }
            .at(self, x_satay_pointer));
        };

        let item = self
            .document
            .resolve_schema(items, context)
            .map_err(|error| error.at(self, x_satay_pointer))?;
        let Some(item_object) = item.as_object() else {
            return Err(ValidationError::SatayOutputExpectedObject {
                context: context.to_owned(),
                selector: "map-field",
            }
            .at(self, x_satay_pointer));
        };
        let (item_type, _) = schema_type_and_nullable(item_object, context)
            .map_err(|error| error.at(self, x_satay_pointer))?;
        if !matches!(item_type, Some(OasSchemaType::Object) | None)
            || item_object.properties.is_empty()
        {
            return Err(ValidationError::SatayOutputExpectedObject {
                context: context.to_owned(),
                selector: "map-field",
            }
            .at(self, x_satay_pointer));
        }
        let Some(mapped) = item_object.properties.get(map_field) else {
            return Err(ValidationError::UnknownSatayOutputField {
                context: context.to_owned(),
                selector: "map-field",
                field: map_field.to_owned(),
            }
            .at(self, x_satay_pointer));
        };

        let array_pointer = self.schema_chain_pointer(unwrapped, &unwrapped_pointer, context)?;
        let items_pointer = child_pointer(&array_pointer, "items");
        let item_pointer = self.schema_chain_pointer(items, &items_pointer, context)?;
        let mapped_pointer = child_pointer(&child_pointer(&item_pointer, "properties"), map_field);
        let mapped_use =
            self.schema_use(mapped, &mapped_pointer, SchemaPosition::Value, context)?;

        let output = SchemaUse {
            ty: TypeExpr::Array(ArraySchema {
                items: Box::new(mapped_use),
                constraints: ArrayConstraints {
                    min_items: array.min_items,
                    max_items: array.max_items,
                },
            }),
            nullable: array_nullable,
            annotations: SchemaAnnotations {
                description: optional_description(&array.description),
                format: array.format.clone(),
                default: declared_default(array, &array_pointer, self.presence),
                source: Some(source_ref(self.document_id, &array_pointer)),
            },
        };

        Ok(ResponseProjection {
            selector: OutputSelector {
                unwrap_field: unwrap_field.to_owned(),
                map_field: Some(map_field.to_owned()),
            },
            output,
        })
    }

    /// Locates a Reference Object's terminal declaration after resolution.
    fn component_pointer<T: ResolvableComponent>(
        &self,
        component: &ObjectOrReference<T>,
        fallback: &str,
        context: &str,
    ) -> Result<String, NormalizeError> {
        let mut current = component;
        let mut pointer = fallback.to_owned();
        let mut visited = BTreeSet::new();
        while let ObjectOrReference::Ref { ref_path, .. } = current {
            (current, pointer) =
                self.component_target(ref_path, &pointer, context, &mut visited)?;
        }
        Ok(pointer)
    }

    /// Path Item `$ref` fields and component Reference Objects share a chain.
    /// Like the resolver, this follows the target and never merges siblings.
    fn path_item_pointer(
        &self,
        path_item: &OasPathItem,
        fallback: &str,
        context: &str,
    ) -> Result<String, NormalizeError> {
        let mut reference = path_item.reference.as_deref();
        let mut pointer = fallback.to_owned();
        let mut visited = BTreeSet::new();
        while let Some(raw_reference) = reference {
            let (target, target_pointer) = self.component_target::<OasPathItem>(
                raw_reference,
                &pointer,
                context,
                &mut visited,
            )?;
            pointer = target_pointer;
            reference = match target {
                ObjectOrReference::Object(path_item) => path_item.reference.as_deref(),
                ObjectOrReference::Ref { ref_path, .. } => Some(ref_path),
            };
        }
        Ok(pointer)
    }

    fn component_target<T: ResolvableComponent>(
        &self,
        reference: &str,
        pointer: &str,
        context: &str,
        visited: &mut BTreeSet<String>,
    ) -> Result<(&ObjectOrReference<T>, String), NormalizeError> {
        let parsed = LocalComponentRef::<T>::parse(reference).map_err(|_| {
            ValidationError::InvalidComponentReference {
                reference: reference.to_owned(),
                section: T::COMPONENT_SECTION,
            }
            .at(self, pointer)
        })?;
        if !visited.insert(parsed.name().to_owned()) {
            return Err(ValidationError::CircularReference {
                reference: reference.to_owned(),
            }
            .at(self, pointer));
        }
        let target = self
            .document
            .spec
            .components
            .as_ref()
            .and_then(|components| T::component(components, parsed.name()))
            .ok_or_else(|| {
                ValidationError::ResolveReference {
                    reference: reference.to_owned(),
                    context: context.to_owned(),
                    source: Box::new(ValidationError::MissingJsonPointerToken {
                        token: parsed.name().to_owned(),
                    }),
                }
                .at(self, pointer)
            })?;
        Ok((
            target,
            child_pointer(
                &child_pointer("/components", parsed.section()),
                parsed.name(),
            ),
        ))
    }

    /// Follows a schema `$ref` chain to its terminal inline schema and
    /// returns that terminal's physical pointer.
    ///
    /// The chain order mirrors the resolver's `resolve_schema`, so the
    /// pointer always names the node the resolved value physically lives at.
    fn schema_chain_pointer(
        &self,
        schema: &OasSchema,
        fallback: &str,
        context: &str,
    ) -> Result<String, NormalizeError> {
        let mut current = schema;
        let mut pointer = fallback.to_owned();
        let mut visited = BTreeSet::new();
        while let Some(reference) = current.reference() {
            let parsed =
                schema_component_ref(reference).map_err(|error| error.at(self, &pointer))?;
            if !visited.insert(parsed.name().to_owned()) {
                return Err(ValidationError::CircularReference {
                    reference: reference.to_owned(),
                }
                .at(self, &pointer));
            }
            let Some(target) = self
                .document
                .spec
                .components
                .as_ref()
                .and_then(|components| components.schemas.get(parsed.name()))
            else {
                return Err(ValidationError::ResolveReference {
                    reference: parsed.as_str().to_owned(),
                    context: context.to_owned(),
                    source: Box::new(ValidationError::MissingJsonPointerToken {
                        token: parsed.name().to_owned(),
                    }),
                }
                .at(self, &pointer));
            };
            pointer = child_pointer("/components/schemas", parsed.name());
            current = target;
        }
        Ok(pointer)
    }

    /// Converts every declared security scheme in map order.
    fn security_schemes(&self) -> Result<Vec<SecurityScheme>, NormalizeError> {
        let Some(components) = self.document.spec.components.as_ref() else {
            return Ok(vec![]);
        };

        let mut converted = Vec::with_capacity(components.security_schemes.len());

        for (name, scheme) in &components.security_schemes {
            let pointer = child_pointer("/components/securitySchemes", name);
            let resolved = self
                .document
                .resolve(scheme, &format!("security scheme `{name}`"))
                .map_err(|error| error.at(self, &pointer))?;

            let kind = match resolved {
                OasSecurityScheme::ApiKey {
                    name: wire_name,
                    location,
                    ..
                } => match location.as_str() {
                    "query" => SecuritySchemeKind::ApiKey {
                        wire_name: wire_name.clone(),
                        location: ApiKeyLocation::Query,
                    },
                    "header" => SecuritySchemeKind::ApiKey {
                        wire_name: wire_name.clone(),
                        location: ApiKeyLocation::Header,
                    },
                    "cookie" => SecuritySchemeKind::ApiKey {
                        wire_name: wire_name.clone(),
                        location: ApiKeyLocation::Cookie,
                    },
                    other => {
                        return Err(NormalizeError::ApiKeyLocation {
                            value: other.to_owned(),
                            location: source_ref(
                                self.document_id,
                                &self.component_pointer(
                                    scheme,
                                    &pointer,
                                    &format!("security scheme `{name}`"),
                                )?,
                            ),
                        });
                    }
                },
                OasSecurityScheme::Http {
                    scheme,
                    bearer_format,
                    ..
                } => SecuritySchemeKind::Http {
                    scheme: scheme.clone(),
                    bearer_format: bearer_format.clone(),
                },
                OasSecurityScheme::OAuth2 { flows, .. } => SecuritySchemeKind::OAuth2 {
                    flows: map_flows(flows),
                },
                OasSecurityScheme::OpenIdConnect {
                    open_id_connect_url,
                    ..
                } => SecuritySchemeKind::OpenIdConnect {
                    url: open_id_connect_url.clone(),
                },
                OasSecurityScheme::MutualTls { .. } => SecuritySchemeKind::MutualTls,
            };

            converted.push(SecurityScheme {
                name: name.clone(),
                description: optional_description(scheme_description(resolved)),
                kind,
            });
        }

        Ok(converted)
    }
}

/// Returns the declared default of an object schema, distinguishing an
/// explicit JSON null from an absent default through the presence index.
fn declared_default(
    object: &ObjectSchema,
    pointer: &str,
    presence: &PresenceIndex,
) -> Option<serde_json::Value> {
    match object.default.clone() {
        Some(value) => Some(value),
        None if presence.has(pointer, "default") => Some(Value::Null),
        None => None,
    }
}

/// Maps one declared server with its variables in map order.
fn map_server(server: &OasServer) -> Server {
    Server {
        url: server.url.clone(),
        description: optional_description(&server.description),
        variables: server
            .variables
            .iter()
            .map(|(name, variable)| ServerVariable {
                name: name.clone(),
                default: variable.default.clone(),
                enum_values: variable.substitutions_enum.clone(),
                description: optional_description(&variable.description),
            })
            .collect(),
    }
}

fn map_servers(servers: &[OasServer]) -> Vec<Server> {
    servers.iter().map(map_server).collect()
}

/// Maps one security requirement alternative; an empty requirement maps to
/// the anonymous alternative with no schemes.
fn requirement_alternative(requirement: &OasSecurityRequirement) -> SecurityRequirement {
    SecurityRequirement {
        schemes: requirement
            .0
            .iter()
            .map(|(scheme, scopes)| SecurityRequirementScheme {
                scheme: scheme.clone(),
                scopes: scopes.clone(),
            })
            .collect(),
    }
}

/// Maps the declared OAuth2 flows in the struct's fixed order.
fn map_flows(flows: &OasFlows) -> Vec<OAuthFlow> {
    let mut mapped = vec![];

    if let Some(flow) = flows.implicit.as_ref() {
        mapped.push(OAuthFlow {
            kind: OAuthFlowKind::Implicit,
            authorization_url: Some(flow.authorization_url.as_str().to_owned()),
            token_url: None,
            refresh_url: flow.refresh_url.as_ref().map(|url| url.as_str().to_owned()),
            scopes: map_scopes(&flow.scopes),
        });
    }

    if let Some(flow) = flows.password.as_ref() {
        mapped.push(OAuthFlow {
            kind: OAuthFlowKind::Password,
            authorization_url: None,
            token_url: Some(flow.token_url.as_str().to_owned()),
            refresh_url: flow.refresh_url.as_ref().map(|url| url.as_str().to_owned()),
            scopes: map_scopes(&flow.scopes),
        });
    }

    if let Some(flow) = flows.client_credentials.as_ref() {
        mapped.push(OAuthFlow {
            kind: OAuthFlowKind::ClientCredentials,
            authorization_url: None,
            token_url: Some(flow.token_url.as_str().to_owned()),
            refresh_url: flow.refresh_url.as_ref().map(|url| url.as_str().to_owned()),
            scopes: map_scopes(&flow.scopes),
        });
    }

    if let Some(flow) = flows.authorization_code.as_ref() {
        mapped.push(OAuthFlow {
            kind: OAuthFlowKind::AuthorizationCode,
            authorization_url: Some(flow.authorization_url.as_str().to_owned()),
            token_url: Some(flow.token_url.as_str().to_owned()),
            refresh_url: flow.refresh_url.as_ref().map(|url| url.as_str().to_owned()),
            scopes: map_scopes(&flow.scopes),
        });
    }

    mapped
}

fn map_scopes(scopes: &OasMap<String, String>) -> Vec<OAuthScope> {
    scopes
        .iter()
        .map(|(name, description)| OAuthScope {
            name: name.clone(),
            description: description.clone(),
        })
        .collect()
}

/// Returns the description carried by any security scheme variant.
fn scheme_description(scheme: &OasSecurityScheme) -> &Option<String> {
    match scheme {
        OasSecurityScheme::ApiKey { description, .. }
        | OasSecurityScheme::Http { description, .. }
        | OasSecurityScheme::OAuth2 { description, .. }
        | OasSecurityScheme::OpenIdConnect { description, .. }
        | OasSecurityScheme::MutualTls { description, .. } => description,
    }
}

fn style_wire(style: OasParameterStyle) -> ParameterStyle {
    match style {
        OasParameterStyle::Matrix => ParameterStyle::Matrix,
        OasParameterStyle::Label => ParameterStyle::Label,
        OasParameterStyle::Form => ParameterStyle::Form,
        OasParameterStyle::Simple => ParameterStyle::Simple,
        OasParameterStyle::SpaceDelimited => ParameterStyle::SpaceDelimited,
        OasParameterStyle::PipeDelimited => ParameterStyle::PipeDelimited,
        OasParameterStyle::DeepObject => ParameterStyle::DeepObject,
    }
}
