use super::*;

#[test]
fn unmapped_diagnostics_remain_recoverable() {
    let diagnostic = Diagnostic {
        kind: DiagnosticKind::ApiKeyLocation {
            value: "cookie".to_owned(),
            location: satay_ir::SourceRef {
                document: "input.yaml".to_owned(),
                pointer: "/components/securitySchemes/session/in".to_owned(),
            },
        },
        message: "unsupported API key location".to_owned(),
    };

    assert!(matches!(
        try_restore(diagnostic),
        Err(DiagnosticKind::ApiKeyLocation { value, .. }) if value == "cookie"
    ));
}
