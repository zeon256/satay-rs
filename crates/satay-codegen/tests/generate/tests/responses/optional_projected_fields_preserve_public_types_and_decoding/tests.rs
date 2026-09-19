use super::generated::*;

fn response(body: &[u8]) -> satay_runtime::ResponseParts<Vec<u8>> {
    satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: body.to_vec(),
    }
}

#[test]
fn missing_unwrapped_and_mapped_values_remain_optional() {
    for body in [b"{}".as_slice(), br#"{"value":null}"#] {
        let decoded: GetServicesResponse =
            operations::get_services::decode_get_services_response(response(body).as_bytes())
                .unwrap();
        assert!(matches!(decoded, GetServicesResponse::Ok(None)));
        let decoded: GetLinksResponse =
            operations::get_links::decode_get_links_response(response(body).as_bytes()).unwrap();
        assert!(matches!(decoded, GetLinksResponse::Ok(None)));
    }
    let decoded: GetLinksResponse = operations::get_links::decode_get_links_response(
        response(br#"{"value":[{}, {"Link":null}, {"Link":"found"}]}"#).as_bytes(),
    )
    .unwrap();
    let GetLinksResponse::Ok(Some(values)) = decoded else {
        panic!("projected links")
    };
    assert_eq!(values, vec![None, None, Some("found".to_owned())]);
}
