use oas3::spec::{
    LocalComponentRef, ObjectSchema as OasObjectSchema, Schema as OasSchema,
    SchemaType as OasSchemaType, SchemaTypeSet as OasSchemaTypeSet,
};

use crate::error::ValidationError;
use crate::ident::type_ident;

pub(super) fn schema_ref_type_name(reference: &str) -> Result<String, ValidationError> {
    let reference = schema_component_ref(reference)?;
    Ok(type_ident(reference.name()))
}

pub(super) fn schema_component_ref(
    reference: &str,
) -> Result<LocalComponentRef<'_, OasSchema>, ValidationError> {
    LocalComponentRef::parse(reference).map_err(|source| {
        ValidationError::InvalidComponentReference {
            reference: source.reference().to_owned(),
            section: "schemas",
        }
    })
}

pub(super) fn schema_type_and_nullable(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(Option<OasSchemaType>, bool), ValidationError> {
    let Some(schema_type) = schema.schema_type.as_ref() else {
        return Ok((None, false));
    };

    match schema_type {
        OasSchemaTypeSet::Single(OasSchemaType::Null) => {
            Err(ValidationError::UnsupportedSchemaType {
                context: context.to_owned(),
                kind: "null".to_owned(),
            })
        }
        OasSchemaTypeSet::Single(schema_type) => Ok((Some(*schema_type), false)),
        OasSchemaTypeSet::Multiple(types) => {
            let mut nullable = false;
            let mut non_null = None;
            let mut non_null_count = 0usize;
            for schema_type in types {
                if *schema_type == OasSchemaType::Null {
                    nullable = true;
                } else {
                    non_null_count += 1;
                    if non_null.is_none() {
                        non_null = Some(*schema_type);
                    }
                }
            }

            match non_null_count {
                0 => Err(ValidationError::UnsupportedSchemaType {
                    context: context.to_owned(),
                    kind: "null".to_owned(),
                }),
                1 => Ok((non_null, nullable)),
                _ => Err(ValidationError::MultipleNonNullSchemaTypesUnsupported {
                    context: context.to_owned(),
                }),
            }
        }
    }
}

pub(super) fn schema_type_wire(schema_type: OasSchemaType) -> &'static str {
    match schema_type {
        OasSchemaType::Boolean => "boolean",
        OasSchemaType::Integer => "integer",
        OasSchemaType::Number => "number",
        OasSchemaType::String => "string",
        OasSchemaType::Array => "array",
        OasSchemaType::Object => "object",
        OasSchemaType::Null => "null",
    }
}

pub(super) fn reject_one_of(
    schema: &OasObjectSchema,
    context: &str,
) -> Result<(), ValidationError> {
    if !schema.one_of.is_empty() {
        return Err(ValidationError::UnsupportedComposition {
            context: context.to_owned(),
            keyword: "oneOf",
        });
    }
    Ok(())
}
