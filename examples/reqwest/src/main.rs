include!(concat!(env!("OUT_DIR"), "/satay_generated.rs"));

use generated::{Api, GetBusArrivalAction, GetBusArrivalResponse};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let account_key = std::env::var("LTA_ACCOUNT_KEY")?;
    let mut args = std::env::args().skip(1);
    let bus_stop_code = args.next().unwrap_or_else(|| "83139".to_owned());
    let service_no = args.next();

    let api = Api::new().account_key(account_key);
    let mut action = api.get_bus_arrival(bus_stop_code);
    if let Some(service_no) = service_no {
        action = action.service_no(service_no);
    }

    let request: reqwest::Request = action.request()?.try_into()?;
    let mut response = reqwest::Client::new().execute(request).await?;

    let response = satay_runtime::ResponseParts {
        status: response.status(),
        headers: std::mem::take(response.headers_mut()),
        body: response.bytes().await?.to_vec(),
    };

    match GetBusArrivalAction::decode(response)? {
        GetBusArrivalResponse::Ok(arrival) => {
            println!(
                "{:?}", arrival
            );
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
