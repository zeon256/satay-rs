use super::generated::*;

#[test]
fn decodes_nonrecursive_collision_response() {
    let expected = PsiResponse { value: 42 };
    let manual = PsiOperationResponse::Ok(expected.clone());
    assert_eq!(manual, PsiOperationResponse::Ok(expected));

    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{"value":42}"#.to_vec(),
    };
    let decoded = PsiAction::<satay_runtime::storage::AllocStorage>::decode(response.as_bytes()).expect("decoded response");

    assert_eq!(decoded, PsiOperationResponse::Ok(PsiResponse { value: 42 }));
}
