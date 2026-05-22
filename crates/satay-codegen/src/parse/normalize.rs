use std::collections::BTreeMap;

use oas3::{
    Map as OasMap,
    spec::{
        MediaType as OasMediaType, ObjectOrReference, ObjectSchema as OasObjectSchema,
        Operation as OasOperation, Parameter as OasParameter, PathItem as OasPathItem,
        RequestBody as OasRequestBody, Response as OasResponse, Schema as OasSchema,
    },
};

use super::reference::{
    resolve_parameter, resolve_path_item, resolve_request_body, resolve_response,
};
use super::resolve::ResolvedDocument;
use crate::error::ValidationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SchemaId(usize);

impl SchemaId {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NormalizedSchema<'a> {
    pub(crate) schema: &'a OasObjectSchema,
}

#[derive(Debug)]
pub(crate) struct NormalizedSchemas<'a> {
    ids: BTreeMap<SchemaKey, SchemaId>,
    schemas: BTreeMap<SchemaId, NormalizedSchema<'a>>,
}

impl<'a> NormalizedSchemas<'a> {
    pub(crate) fn new() -> Self {
        Self {
            ids: BTreeMap::new(),
            schemas: BTreeMap::new(),
        }
    }

    pub(crate) fn object_id(&self, schema: &OasObjectSchema, context: &str) -> SchemaId {
        self.ids
            .get(&SchemaKey::new(schema))
            .copied()
            .unwrap_or_else(|| panic!("schema id missing during validation/lowering for {context}"))
    }

    pub(crate) fn schema_id(&self, schema: &OasSchema, context: &str) -> Option<SchemaId> {
        match schema {
            OasSchema::Boolean(_) => None,
            OasSchema::Object(object) => match object.as_ref() {
                ObjectOrReference::Object(schema) => Some(self.object_id(schema, context)),
                ObjectOrReference::Ref { .. } => None,
            },
        }
    }

    pub(crate) fn schema(&self, id: SchemaId, context: &str) -> NormalizedSchema<'a> {
        self.schemas
            .get(&id)
            .copied()
            .unwrap_or_else(|| panic!("normalized schema missing for {context}"))
    }

    fn insert(&mut self, schema: &'a OasObjectSchema) -> SchemaId {
        let key = SchemaKey::new(schema);
        if let Some(id) = self.ids.get(&key).copied() {
            return id;
        }

        let id = SchemaId::new(self.schemas.len());
        self.ids.insert(key, id);
        self.schemas.insert(id, NormalizedSchema { schema });
        id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SchemaKey(usize);

impl SchemaKey {
    fn new(schema: &OasObjectSchema) -> Self {
        Self(schema as *const OasObjectSchema as usize)
    }
}

#[derive(Debug)]
pub(crate) struct NormalizedDocument<'a> {
    pub(crate) resolved: ResolvedDocument<'a>,
    pub(crate) schemas: NormalizedSchemas<'a>,
}

pub(crate) fn normalize_document<'a>(
    resolved: ResolvedDocument<'a>,
) -> Result<NormalizedDocument<'a>, ValidationError> {
    let mut normalizer = SchemaNormalizer {
        schemas: NormalizedSchemas::new(),
    };

    normalizer.normalize_components(&resolved)?;
    normalizer.normalize_paths(&resolved)?;

    Ok(NormalizedDocument {
        resolved,
        schemas: normalizer.schemas,
    })
}

struct SchemaNormalizer<'a> {
    schemas: NormalizedSchemas<'a>,
}

impl<'a> SchemaNormalizer<'a> {
    fn normalize_components(
        &mut self,
        document: &ResolvedDocument<'a>,
    ) -> Result<(), ValidationError> {
        let Some(components) = document.spec.components.as_ref() else {
            return Ok(());
        };

        for (schema_name, schema) in &components.schemas {
            self.normalize_schema(schema, &format!("schema `{schema_name}`"))?;
        }

        Ok(())
    }

    fn normalize_paths(&mut self, document: &ResolvedDocument<'a>) -> Result<(), ValidationError> {
        let Some(paths) = document.spec.paths.as_ref() else {
            return Ok(());
        };

        for (path, path_item) in paths {
            let path_item = resolve_path_item(document, path_item, &format!("path item `{path}`"))?;
            self.normalize_path_item(document, path_item, &format!("path item `{path}`"))?;
        }

        Ok(())
    }

    fn normalize_path_item(
        &mut self,
        document: &ResolvedDocument<'a>,
        path_item: &'a OasPathItem,
        context: &str,
    ) -> Result<(), ValidationError> {
        for parameter in &path_item.parameters {
            self.normalize_parameter(document, parameter, &format!("{context}.parameters"))?;
        }

        self.normalize_operation(document, path_item.get.as_ref(), &format!("{context}.get"))?;
        self.normalize_operation(
            document,
            path_item.post.as_ref(),
            &format!("{context}.post"),
        )?;
        self.normalize_operation(document, path_item.put.as_ref(), &format!("{context}.put"))?;
        self.normalize_operation(
            document,
            path_item.patch.as_ref(),
            &format!("{context}.patch"),
        )?;
        self.normalize_operation(
            document,
            path_item.delete.as_ref(),
            &format!("{context}.delete"),
        )?;
        self.normalize_operation(
            document,
            path_item.head.as_ref(),
            &format!("{context}.head"),
        )?;
        self.normalize_operation(
            document,
            path_item.options.as_ref(),
            &format!("{context}.options"),
        )?;
        self.normalize_operation(
            document,
            path_item.trace.as_ref(),
            &format!("{context}.trace"),
        )?;

        Ok(())
    }

    fn normalize_operation(
        &mut self,
        document: &ResolvedDocument<'a>,
        operation: Option<&'a OasOperation>,
        context: &str,
    ) -> Result<(), ValidationError> {
        let Some(operation) = operation else {
            return Ok(());
        };

        let operation_context = operation
            .operation_id
            .as_ref()
            .map(|operation_id| format!("operation `{operation_id}`"))
            .unwrap_or_else(|| context.to_owned());

        for parameter in &operation.parameters {
            self.normalize_parameter(
                document,
                parameter,
                &format!("{operation_context} parameters"),
            )?;
        }

        if let Some(request_body) = operation.request_body.as_ref() {
            self.normalize_request_body(
                document,
                request_body,
                &format!("{operation_context} requestBody"),
            )?;
        }

        if let Some(responses) = operation.responses.as_ref() {
            for (status, response) in responses {
                self.normalize_response(
                    document,
                    response,
                    &format!("{operation_context} responses {status}"),
                )?;
            }
        }

        Ok(())
    }

    fn normalize_parameter(
        &mut self,
        document: &ResolvedDocument<'a>,
        parameter: &'a ObjectOrReference<OasParameter>,
        context: &str,
    ) -> Result<(), ValidationError> {
        let parameter = resolve_parameter(document, parameter, context)?;
        if let Some(schema) = parameter.schema.as_ref() {
            self.normalize_schema(schema, &format!("{context}.schema"))?;
        }

        Ok(())
    }

    fn normalize_request_body(
        &mut self,
        document: &ResolvedDocument<'a>,
        request_body: &'a ObjectOrReference<OasRequestBody>,
        context: &str,
    ) -> Result<(), ValidationError> {
        let request_body = resolve_request_body(document, request_body, context)?;
        self.normalize_content(&request_body.content, &format!("{context}.content"))
    }

    fn normalize_response(
        &mut self,
        document: &ResolvedDocument<'a>,
        response: &'a ObjectOrReference<OasResponse>,
        context: &str,
    ) -> Result<(), ValidationError> {
        let response = resolve_response(document, response, context)?;
        self.normalize_content(&response.content, &format!("{context}.content"))
    }

    fn normalize_content(
        &mut self,
        content: &'a OasMap<String, OasMediaType>,
        context: &str,
    ) -> Result<(), ValidationError> {
        for (media_type, media) in content {
            if let Some(schema) = media.schema.as_ref() {
                self.normalize_schema(schema, &format!("{context}.{media_type}.schema"))?;
            }
        }

        Ok(())
    }

    fn normalize_schema(
        &mut self,
        schema: &'a OasSchema,
        context: &str,
    ) -> Result<(), ValidationError> {
        match schema {
            OasSchema::Boolean(_) => Ok(()),
            OasSchema::Object(schema) => match schema.as_ref() {
                ObjectOrReference::Object(schema) => self.normalize_object_schema(schema, context),
                ObjectOrReference::Ref { .. } => Ok(()),
            },
        }
    }

    fn normalize_object_schema(
        &mut self,
        schema: &'a OasObjectSchema,
        context: &str,
    ) -> Result<(), ValidationError> {
        self.schemas.insert(schema);

        for (property_name, property_schema) in &schema.properties {
            self.normalize_schema(
                property_schema,
                &format!("{context}.properties.{property_name}"),
            )?;
        }

        if let Some(items) = schema.items.as_deref() {
            self.normalize_schema(items, &format!("{context}.items"))?;
        }

        for (keyword, schemas) in [
            ("oneOf", &schema.one_of),
            ("anyOf", &schema.any_of),
            ("allOf", &schema.all_of),
        ] {
            for (index, schema) in schemas.iter().enumerate() {
                self.normalize_schema(schema, &format!("{context}.{keyword}[{index}]"))?;
            }
        }

        Ok(())
    }
}
