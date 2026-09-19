use super::generated::*;

#[test]
fn encodes_optional_date_query_parameter() {
    let day = satay_runtime::parse_date("2024-07-16").unwrap();
    let parts = operations::psi::psi_parts(PsiInput::new().date(day)).expect("request parts");
    assert_eq!(parts.uri, "/psi?date=2024-07-16");
}
