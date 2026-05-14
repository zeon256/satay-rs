include!(concat!(env!("OUT_DIR"), "/satay_generated.rs"));

use std::env;
use std::error::Error;
use std::mem;

use generated::{Api, GetBusArrivalAction, GetBusArrivalResponse};

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

    let request: http::Request<Vec<u8>> = action.request()?;

    let url = request.uri().to_string();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();

    let mut ureq_request = agent.get(&url);
    for (name, value) in request.headers() {
        ureq_request = ureq_request.header(name.as_str(), value.to_str()?);
    }

    let mut ureq_response = ureq_request.call()?;

    let status = ureq_response.status();
    let headers = mem::take(ureq_response.headers_mut());
    let body = ureq_response.body_mut().read_to_vec()?;

    let response = satay_runtime::ResponseParts {
        status,
        headers,
        body,
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
