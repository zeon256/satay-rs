use super::*;
use crate::generated::GetBusArrivalAction;

#[test]
fn tests_the_api_without_http() -> Result<(), Box<dyn Error>> {
    let api = Api::new().base_url("").account_key("test-key");
    let action = api
        .bus()
        .get_arrival(83139)
        .service_no(BusServiceNumber::try_new("15")?);

    let request = action.request()?;
    assert_eq!(request.method(), http::Method::GET);
    assert_eq!(request.uri().path(), "/v3/BusArrival");
    assert_eq!(request.headers()["AccountKey"], "test-key");

    let query = request.uri().query().unwrap_or_default();
    assert!(query.contains("BusStopCode=83139"));
    assert!(query.contains("ServiceNo=15"));

    let response = GetBusArrivalAction::<satay_runtime::storage::AllocStorage>::decode(
        satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{
                "odata.metadata": "https://datamall2.mytransport.sg/ltaodataservice/v3/BusArrival",
                "BusStopCode": "83139",
                "Services": []
            }"#,
        },
    )?;

    let GetBusArrivalResponse::Ok(arrival) = response else {
        panic!("expected 200 OK bus arrival response");
    };
    assert_eq!(
        arrival.odata_metadata.host_str(),
        Some("datamall2.mytransport.sg")
    );
    assert_eq!(arrival.odata_metadata.scheme(), "https");
    assert_eq!(arrival.bus_stop_code, 83139);
    assert!(arrival.services.is_empty());

    Ok(())
}
