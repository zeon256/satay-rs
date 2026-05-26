use crate::error::ValidationError;

pub(crate) fn local_ref_name(
    reference: &str,
    section: &'static str,
) -> Result<String, ValidationError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_invalid_component_reference(reference: &str, section: &'static str) {
        match local_ref_name(reference, section) {
            Err(ValidationError::InvalidComponentReference {
                reference: actual,
                section: actual_section,
            }) => {
                assert_eq!(actual, reference);
                assert_eq!(actual_section, section);
            }
            other => panic!("unexpected result for {reference}: {other:?}"),
        }
    }

    #[test]
    fn extracts_local_component_reference_names() {
        assert_eq!(
            local_ref_name("#/components/schemas/User", "schemas").expect("local ref"),
            "User"
        );
        assert_eq!(
            local_ref_name("#/components/parameters/userId", "parameters").expect("local ref"),
            "userId"
        );
    }

    #[test]
    fn unescapes_json_pointer_tokens() {
        assert_eq!(
            local_ref_name("#/components/schemas/User~0Code", "schemas").expect("local ref"),
            "User~Code"
        );
        assert_eq!(
            local_ref_name("#/components/schemas/User~1Code", "schemas").expect("local ref"),
            "User/Code"
        );
        assert_eq!(
            local_ref_name("#/components/schemas/a~1b~0c", "schemas").expect("local ref"),
            "a/b~c"
        );
    }

    #[test]
    fn rejects_external_and_wrong_section_references() {
        assert_invalid_component_reference("https://example.test/openapi.yaml#/User", "schemas");
        assert_invalid_component_reference("#/paths/~1users/get", "schemas");
        assert_invalid_component_reference("#/components/responses/User", "schemas");
        assert_invalid_component_reference("#/components/schemas", "schemas");
    }
}
