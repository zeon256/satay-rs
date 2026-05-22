use std::collections::BTreeMap;

use oas3::spec::{ObjectOrReference, ObjectSchema as OasObjectSchema, Schema as OasSchema};

use super::index::{SchemaId, SchemaIndex};
use super::satay::ValidatedSataySchema;
use crate::model::{IntegerType, Validation};

#[derive(Debug)]
pub(crate) struct ValidatedSchemas {
    index: SchemaIndex,
    schemas: BTreeMap<SchemaId, ValidatedSchema>,
}

impl ValidatedSchemas {
    pub(super) fn new(index: SchemaIndex) -> Self {
        Self {
            index,
            schemas: BTreeMap::new(),
        }
    }

    pub(super) fn insert_schema(
        &mut self,
        schema: &OasObjectSchema,
        context: &str,
        validated: ValidatedSchema,
    ) {
        let id = self.index.id(schema, context);
        self.schemas.insert(id, validated);
    }

    pub(crate) fn schema(&self, schema: &OasObjectSchema, context: &str) -> &ValidatedSchema {
        let id = self.index.id(schema, context);
        self.schemas.get(&id).unwrap_or_else(|| {
            panic!("validated schema data missing during lowering for {context}")
        })
    }

    pub(crate) fn treat_error_as_none(&self, schema: &OasSchema, context: &str) -> bool {
        match schema {
            OasSchema::Boolean(_) => false,
            OasSchema::Object(object) => match object.as_ref() {
                ObjectOrReference::Object(schema) => {
                    self.schema(schema, context).satay.treat_error_as_none
                }
                ObjectOrReference::Ref { .. } => false,
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ValidatedSchema {
    pub(crate) satay: ValidatedSataySchema,
    pub(crate) integer_type: Option<IntegerType>,
    pub(crate) validation: Option<Validation>,
}
