use super::generated::*;

#[test]
fn unwraps_value_payload() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{
            "odata.metadata":"https://example.test/metadata",
            "value":[
                {"id":"10","name":"Airport Express"},
                {"id":"20","name":"City Loop"}
            ]
        }"#
        .to_vec(),
    };
    let decoded: GetServicesResponse =
        operations::get_services::decode_get_services_response(response.as_bytes())
            .expect("projected services");

    match decoded {
        GetServicesResponse::Ok(services) => {
            assert_eq!(services.len(), 2);
            assert_eq!(services[0].id, "10");
            assert_eq!(services[1].name, "City Loop");
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn unwraps_and_maps_link_payload() {
    let response = satay_runtime::ResponseParts {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: br#"{
            "value":[
                {"Link":"https://example.test/a","Description":"A"},
                {"Link":"https://example.test/b","Description":"B"}
            ]
        }"#
        .to_vec(),
    };
    let decoded: GetLinksResponse =
        operations::get_links::decode_get_links_response(response.as_bytes())
            .expect("projected links");

    match decoded {
        GetLinksResponse::Ok(links) => assert_eq!(
            links,
            vec![
                "https://example.test/a".to_owned(),
                "https://example.test/b".to_owned(),
            ]
        ),
        other => panic!("unexpected response: {other:?}"),
    }
}
