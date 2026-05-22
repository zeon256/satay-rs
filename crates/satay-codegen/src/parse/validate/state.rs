use std::collections::BTreeMap;

use oas3::spec::{ObjectOrReference, ObjectSchema as OasObjectSchema, Schema as OasSchema};

use super::satay::ValidatedSataySchema;
use crate::model::{IntegerType, Validation};

#[derive(Debug, Default)]
pub(crate) struct ValidatedSchemas {
    schemas: BTreeMap<SchemaKey, ValidatedSchema>,
}

impl ValidatedSchemas {
    pub(super) fn insert_schema(&mut self, schema: &OasObjectSchema, validated: ValidatedSchema) {
        self.schemas.insert(SchemaKey::new(schema), validated);
    }

    pub(crate) fn schema(&self, schema: &OasObjectSchema, context: &str) -> &ValidatedSchema {
        self.schemas
            .get(&SchemaKey::new(schema))
            .unwrap_or_else(|| {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SchemaKey(usize);

impl SchemaKey {
    fn new(schema: &OasObjectSchema) -> Self {
        Self(schema as *const OasObjectSchema as usize)
    }
}
