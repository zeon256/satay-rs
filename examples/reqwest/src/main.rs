include!(concat!(env!("OUT_DIR"), "/satay_generated.rs"));

use generated::{Api, GetBusArrivalResponse};
use satay_reqwest::{ReqwestActionExt, reqwest};
use std::env;

use crate::generated::BusServiceNumber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = Api::new().account_key(env::var("LTA_ACCOUNT_KEY")?);

    let client = reqwest::Client::new();
    let response = api
        .get_bus_arrival("83139")
        .service_no(BusServiceNumber::try_new("15")?)
        .send_with(&client)
        .await?;

    match response {
        GetBusArrivalResponse::Ok(arrival) => {
            println!("{:?}", arrival);
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
