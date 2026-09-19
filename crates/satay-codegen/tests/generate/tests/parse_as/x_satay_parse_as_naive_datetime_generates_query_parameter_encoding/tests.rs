use super::generated::*;

#[test]
fn encodes_optional_naive_datetime_query_parameter() {
    let at = satay_runtime::parse_naive_datetime("2024-07-16T23:59:00").unwrap();
    let parts = operations::psi::psi_parts(PsiInput::new().date(at)).expect("request parts");
    assert_eq!(parts.uri, "/psi?date=2024-07-16T23%3A59%3A00");
}
