include!(concat!(env!("OUT_DIR"), "/satay_generated.rs"));

use std::error::Error;
use std::{collections::HashSet, env};

use generated::{Api, GetBusServicesResponse};
use satay_reqwest::{ReqwestActionExt, reqwest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let account_key = env::var("LTA_ACCOUNT_KEY")?;
    let mut args = env::args().skip(1);
    let _bus_stop_code = args.next().unwrap_or_else(|| "83139".to_owned());
    let _service_no = args.next();

    let api = Api::new().account_key(account_key);

    let mut all_services = vec![];
    let mut skip = 0;
    let client = reqwest::Client::new();

    loop {
        let action = api.get_bus_services().skip(skip);

        let response = action.send_with(&client).await?;

        match response {
            GetBusServicesResponse::Ok(bus_services_response) => {
                println!("{} services returned", bus_services_response.value.len());
                if bus_services_response.value.is_empty() {
                    break;
                }

                let sz = bus_services_response.value.len();
                skip += sz as u64;
                all_services.extend(bus_services_response.value);

                // the API returns at most 500 items per request
                if sz < 500 {
                    break;
                }
            }
            GetBusServicesResponse::UnexpectedStatus(status_code, body) => {
                eprintln!(
                    "unexpected status {status_code}: {}",
                    String::from_utf8_lossy(&body)
                );
            }
        }
    }

    println!("Total bus services: {}", all_services.len());

    // get all unique bus service numbers
    let unique_service_nos = all_services
        .iter()
        .map(|service| service.service_no.clone())
        .collect::<HashSet<_>>();

    println!(
        "Unique number of bus services: {}",
        unique_service_nos.len()
    );

    // figure out longest bus service number
    let longest_service_no = unique_service_nos
        .iter()
        .max_by_key(|service_no| service_no.len())
        .unwrap();

    println!("Longest bus service number: {}", longest_service_no);

    Ok(())
}
