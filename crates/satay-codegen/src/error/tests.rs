use super::Error;

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
