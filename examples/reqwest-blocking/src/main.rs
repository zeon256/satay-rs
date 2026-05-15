include!(concat!(env!("OUT_DIR"), "/satay_generated.rs"));

use std::env;
use std::error::Error;
use std::mem;

use generated::{Api, GetBusArrivalAction, GetBusArrivalResponse};
use reqwest::blocking;

fn main() -> Result<(), Box<dyn Error>> {
    let account_key = env::var("LTA_ACCOUNT_KEY")?;
    let mut args = env::args().skip(1);
    let bus_stop_code = args.next().unwrap_or_else(|| "83139".to_owned());
    let service_no = args.next();

    let api = Api::new().account_key(account_key);
    let mut action = api.get_bus_arrival(bus_stop_code);
    if let Some(service_no) = service_no {
        action = action.service_no(service_no);
    }

    let request: blocking::Request = action.request()?.try_into()?;
    let mut response = blocking::Client::new().execute(request)?;

    let response = satay_runtime::ResponseParts {
        status: response.status(),
        headers: mem::take(response.headers_mut()),
        body: response.bytes()?.to_vec(),
    };

    match GetBusArrivalAction::decode(response)? {
        GetBusArrivalResponse::Ok(arrival) => {
            println!(
                "{} services for bus stop {}",
                arrival.services.len(),
                arrival.bus_stop_code
            );
            for service in arrival.services.iter().take(8) {
                println!(
                    "{} ({:?}) - Reaching at {:?}",
                    service.service_no, service.operator, service.next_bus
                );
            }
        }
        GetBusArrivalResponse::UnexpectedStatus(status, body) => {
            eprintln!(
                "unexpected status {status}: {}",
                String::from_utf8_lossy(&body)
            );
        }
    }

    Ok(())
}
