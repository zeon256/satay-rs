use std::collections::BTreeMap;

use super::SchemaId;
use super::satay::ValidatedSataySchema;
use crate::model::{IntegerType, Validation};

#[derive(Debug, Default)]
pub(crate) struct ValidatedSchemas {
    schemas: BTreeMap<SchemaId, ValidatedSchema>,
}

impl ValidatedSchemas {
    pub(super) fn insert_schema(&mut self, id: SchemaId, validated: ValidatedSchema) {
        self.schemas.insert(id, validated);
    }

    pub(crate) fn schema(&self, id: SchemaId, context: &str) -> &ValidatedSchema {
        self.schemas.get(&id).unwrap_or_else(|| {
            panic!("validated schema data missing during lowering for {context}")
        })
    }

    pub(crate) fn treat_error_as_none(&self, id: Option<SchemaId>, context: &str) -> bool {
        id.is_some_and(|id| self.schema(id, context).satay.treat_error_as_none)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ValidatedSchema {
    pub(crate) satay: ValidatedSataySchema,
    pub(crate) integer_type: Option<IntegerType>,
    pub(crate) validation: Option<Validation>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::IntegerType;

    #[test]
    fn stores_validated_schema_metadata_by_schema_id() {
        let schema_id = SchemaId::new(7);
        let mut schemas = ValidatedSchemas::default();

        schemas.insert_schema(
            schema_id,
            ValidatedSchema {
                satay: ValidatedSataySchema {
                    treat_error_as_none: true,
                    ..ValidatedSataySchema::default()
                },
                integer_type: Some(IntegerType::U8),
                validation: None,
            },
        );

        assert_eq!(
            schemas.schema(schema_id, "test").integer_type,
            Some(IntegerType::U8)
        );
        assert!(schemas.treat_error_as_none(Some(schema_id), "test"));
        assert!(!schemas.treat_error_as_none(None, "test"));
    }
}
