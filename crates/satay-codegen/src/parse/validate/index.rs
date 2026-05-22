use std::collections::BTreeMap;

use oas3::{
    Map as OasMap,
    spec::{
        MediaType as OasMediaType, ObjectOrReference, ObjectSchema as OasObjectSchema,
        Operation as OasOperation, Parameter as OasParameter, PathItem as OasPathItem,
        RequestBody as OasRequestBody, Response as OasResponse, Schema as OasSchema,
    },
};

use super::super::reference::{
    resolve_parameter, resolve_path_item, resolve_request_body, resolve_response,
};
use super::super::resolve::ResolvedDocument;
use crate::error::ValidationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SchemaId(usize);

#[derive(Debug, Default)]
pub(super) struct SchemaIndex {
    ids: BTreeMap<SchemaKey, SchemaId>,
}

impl SchemaIndex {
    pub(super) fn id(&self, schema: &OasObjectSchema, context: &str) -> SchemaId {
        self.ids
            .get(&SchemaKey::new(schema))
            .copied()
            .unwrap_or_else(|| panic!("schema id missing during validation/lowering for {context}"))
    }

    fn insert(&mut self, schema: &OasObjectSchema) {
        let next = SchemaId(self.ids.len());
        self.ids.entry(SchemaKey::new(schema)).or_insert(next);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SchemaKey(usize);

impl SchemaKey {
    fn new(schema: &OasObjectSchema) -> Self {
        Self(schema as *const OasObjectSchema as usize)
    }
}

pub(super) fn index_document(
    document: &ResolvedDocument<'_>,
) -> Result<SchemaIndex, ValidationError> {
    let mut indexer = SchemaIndexer {
        index: SchemaIndex::default(),
    };

    indexer.index_components(document)?;
    indexer.index_paths(document)?;

    Ok(indexer.index)
}

struct SchemaIndexer {
    index: SchemaIndex,
}

impl SchemaIndexer {
    fn index_components(&mut self, document: &ResolvedDocument<'_>) -> Result<(), ValidationError> {
        let Some(components) = document.spec.components.as_ref() else {
            return Ok(());
        };

        for (schema_name, schema) in &components.schemas {
            self.index_schema(schema, &format!("schema `{schema_name}`"))?;
        }

        Ok(())
    }

    fn index_paths(&mut self, document: &ResolvedDocument<'_>) -> Result<(), ValidationError> {
        let Some(paths) = document.spec.paths.as_ref() else {
            return Ok(());
        };

        for (path, path_item) in paths {
            let path_item = resolve_path_item(document, path_item, &format!("path item `{path}`"))?;
            self.index_path_item(document, path_item, &format!("path item `{path}`"))?;
        }

        Ok(())
    }

    fn index_path_item(
        &mut self,
        document: &ResolvedDocument<'_>,
        path_item: &OasPathItem,
        context: &str,
    ) -> Result<(), ValidationError> {
        for parameter in &path_item.parameters {
            self.index_parameter(document, parameter, &format!("{context}.parameters"))?;
        }

        self.index_operation(document, path_item.get.as_ref(), &format!("{context}.get"))?;
        self.index_operation(
            document,
            path_item.post.as_ref(),
            &format!("{context}.post"),
        )?;
        self.index_operation(document, path_item.put.as_ref(), &format!("{context}.put"))?;
        self.index_operation(
            document,
            path_item.patch.as_ref(),
            &format!("{context}.patch"),
        )?;
        self.index_operation(
            document,
            path_item.delete.as_ref(),
            &format!("{context}.delete"),
        )?;
        self.index_operation(
            document,
            path_item.head.as_ref(),
            &format!("{context}.head"),
        )?;
        self.index_operation(
            document,
            path_item.options.as_ref(),
            &format!("{context}.options"),
        )?;
        self.index_operation(
            document,
            path_item.trace.as_ref(),
            &format!("{context}.trace"),
        )?;

        Ok(())
    }

    fn index_operation(
        &mut self,
        document: &ResolvedDocument<'_>,
        operation: Option<&OasOperation>,
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
            self.index_parameter(
                document,
                parameter,
                &format!("{operation_context} parameters"),
            )?;
        }

        if let Some(request_body) = operation.request_body.as_ref() {
            self.index_request_body(
                document,
                request_body,
                &format!("{operation_context} requestBody"),
            )?;
        }

        if let Some(responses) = operation.responses.as_ref() {
            for (status, response) in responses {
                self.index_response(
                    document,
                    response,
                    &format!("{operation_context} responses {status}"),
                )?;
            }
        }

        Ok(())
    }

    fn index_parameter(
        &mut self,
        document: &ResolvedDocument<'_>,
        parameter: &ObjectOrReference<OasParameter>,
        context: &str,
    ) -> Result<(), ValidationError> {
        let parameter = resolve_parameter(document, parameter, context)?;
        if let Some(schema) = parameter.schema.as_ref() {
            self.index_schema(schema, &format!("{context}.schema"))?;
        }

        Ok(())
    }

    fn index_request_body(
        &mut self,
        document: &ResolvedDocument<'_>,
        request_body: &ObjectOrReference<OasRequestBody>,
        context: &str,
    ) -> Result<(), ValidationError> {
        let request_body = resolve_request_body(document, request_body, context)?;
        self.index_content(&request_body.content, &format!("{context}.content"))
    }

    fn index_response(
        &mut self,
        document: &ResolvedDocument<'_>,
        response: &ObjectOrReference<OasResponse>,
        context: &str,
    ) -> Result<(), ValidationError> {
        let response = resolve_response(document, response, context)?;
        self.index_content(&response.content, &format!("{context}.content"))
    }

    fn index_content(
        &mut self,
        content: &OasMap<String, OasMediaType>,
        context: &str,
    ) -> Result<(), ValidationError> {
        for (media_type, media) in content {
            if let Some(schema) = media.schema.as_ref() {
                self.index_schema(schema, &format!("{context}.{media_type}.schema"))?;
            }
        }

        Ok(())
    }

    fn index_schema(&mut self, schema: &OasSchema, context: &str) -> Result<(), ValidationError> {
        match schema {
            OasSchema::Boolean(_) => Ok(()),
            OasSchema::Object(schema) => match schema.as_ref() {
                ObjectOrReference::Object(schema) => self.index_object_schema(schema, context),
                ObjectOrReference::Ref { .. } => Ok(()),
            },
        }
    }

    fn index_object_schema(
        &mut self,
        schema: &OasObjectSchema,
        context: &str,
    ) -> Result<(), ValidationError> {
        self.index.insert(schema);

        for (property_name, property_schema) in &schema.properties {
            self.index_schema(
                property_schema,
                &format!("{context}.properties.{property_name}"),
            )?;
        }

        if let Some(items) = schema.items.as_deref() {
            self.index_schema(items, &format!("{context}.items"))?;
        }

        for (keyword, schemas) in [
            ("oneOf", &schema.one_of),
            ("anyOf", &schema.any_of),
            ("allOf", &schema.all_of),
        ] {
            for (index, schema) in schemas.iter().enumerate() {
                self.index_schema(schema, &format!("{context}.{keyword}[{index}]"))?;
            }
        }

        Ok(())
    }
}
