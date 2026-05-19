include!(concat!(env!("OUT_DIR"), "/satay_generated.rs"));

mod error;
mod server;
mod transport;
mod wire;

use error::Error;
use generated::{Api, BusServiceNumber, BusStopCode, GetBusArrivalResponse};
use transport::{TungsteniteActionExt, TungsteniteTransport};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let server_addr = server::spawn_local_server().await?;
    let mut transport = TungsteniteTransport::connect(server_addr).await?;

    let api = Api::new()
        .base_url("http://satay.local")
        .account_key("local-dev-key");

    let response = api
        .get_bus_arrival(BusStopCode::try_new("83139").expect("valid bus stop code"))
        .service_no(BusServiceNumber::try_from("15".to_owned()).expect("valid bus service number"))
        .send_over_ws(&mut transport)
        .await?;
    transport.close().await?;

    match response {
        GetBusArrivalResponse::Ok(arrival) => {
            println!(
                "{} services for bus stop {}",
                arrival.services.len(),
                arrival.bus_stop_code
            );
            for service in arrival.services {
                println!(
                    "{} ({:?}) - next bus: {:?}",
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
