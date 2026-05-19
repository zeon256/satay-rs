include!(concat!(env!("OUT_DIR"), "/satay_generated.rs"));

use std::env;
use std::error::Error;

use generated::{Api, GetBusServicesResponse};
use satay_reqwest::{ReqwestActionExt, reqwest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let account_key = env::var("LTA_ACCOUNT_KEY")?;
    let mut args = env::args().skip(1);
    let _bus_stop_code = args.next().unwrap_or_else(|| "83139".to_owned());
    let _service_no = args.next();

    let api = Api::new().account_key(account_key);
    let action = api.get_bus_services();
    // if let Some(service_no) = service_no {
    //     action = action.service_no(service_no);
    // }

    let client = reqwest::Client::new();
    let response = action.send_with(&client).await?;

    match response {
        GetBusServicesResponse::Ok(bus_services_response) => {
            // print how many services are returned
            // and print the first 8 services

            println!("{} services returned", bus_services_response.value.len());

            for service in bus_services_response.value.iter().take(8) {
                println!(
                    "{}: {} - {} ({} - {})",
                    service.service_no,
                    service.operator,
                    service.loop_desc,
                    service.am_offpeak_freq,
                    service.pm_offpeak_freq
                );
            }
        }
        GetBusServicesResponse::UnexpectedStatus(status_code, body) => {
            eprintln!(
                "unexpected status {status_code}: {}",
                String::from_utf8_lossy(&body)
            );
        }
    }

    Ok(())
}
