use super::{Error, ValidationError};
use satay_codegen_rust::ValidationError as BackendValidationError;

#[test]
fn backend_status_class_error_preserves_its_payload_and_message() {
    let backend = BackendValidationError::OutOfRangeStatusClass {
        context: "operation `probe` responses".to_owned(),
        class: 6,
    };
    let message = backend.to_string();
    let facade = ValidationError::from(backend);
    assert_eq!(facade.to_string(), message);
    let ValidationError::OutOfRangeStatusClass { context, class } = facade else {
        panic!("expected status class error");
    };
    assert_eq!(context, "operation `probe` responses");
    assert_eq!(class, 6);
}

#[test]
fn backend_projection_error_preserves_its_payload_and_message() {
    let backend = BackendValidationError::MappedResponseProjectionRequiresArray {
        context: "operation `probe` responses 200 schema".to_owned(),
    };
    let message = backend.to_string();
    let facade = ValidationError::from(backend);
    assert_eq!(facade.to_string(), message);
    let ValidationError::MappedResponseProjectionRequiresArray { context } = facade else {
        panic!("expected projection error");
    };
    assert_eq!(context, "operation `probe` responses 200 schema");
}

#[test]
fn internal_errors_have_stable_context() {
    let error = Error::Internal {
        message: "semantic graph invariant failed".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "internal code generation error: semantic graph invariant failed"
    );
}
