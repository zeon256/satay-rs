include!(concat!(env!("OUT_DIR"), "/satay_generated.rs"));

use generated::{
    Api, BusArrivalTiming, BusServiceArrival, BusServiceNumber, GetBusArrivalResponse,
};
use satay_reqwest::{ReqwestActionExt, reqwest};
use std::{env, error::Error};

const DEFAULT_BUS_STOP_CODE: u32 = 83139;
const DEFAULT_SERVICE_NO: &str = "15";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (bus_stop_code, service_no) = arrival_args()?;
    let api = Api::new().account_key(env::var("LTA_ACCOUNT_KEY")?);

    let client = reqwest::Client::new();
    let response = api
        .get_bus_arrival(bus_stop_code)
        .service_no(service_no)
        .send_with(&client)
        .await?;

    match response {
        GetBusArrivalResponse::Ok(arrival) => {
            if arrival.services.is_empty() {
                println!("bus stop {}: no services returned", arrival.bus_stop_code);
            }
            for service in &arrival.services {
                print_service_arrivals(service);
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

fn arrival_args() -> Result<(u32, BusServiceNumber), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let bus_stop_code = match args.next() {
        Some(value) => value.parse()?,
        None => DEFAULT_BUS_STOP_CODE,
    };
    let service_no = match args.next() {
        Some(value) => BusServiceNumber::try_new(value)?,
        None => BusServiceNumber::try_new(DEFAULT_SERVICE_NO)?,
    };
    if let Some(extra) = args.next() {
        let message = format!(
            "unexpected extra argument `{extra}`; usage: satay-example-reqwest [BUS_STOP_CODE] [SERVICE_NO]"
        );
        return Err(message.into());
    }

    Ok((bus_stop_code, service_no))
}

fn print_service_arrivals(service: &BusServiceArrival) {
    println!("service {} ({})", service.service_no, service.operator);
    print_arrival_slot("next", &service.next_bus);
    print_arrival_slot("next 2", &service.next_bus2);
    print_arrival_slot("next 3", &service.next_bus3);
}

fn print_arrival_slot(label: &str, arrival: &Option<BusArrivalTiming>) {
    let Some(arrival) = arrival else {
        println!("  {label}: no arrival available");
        return;
    };

    println!(
        "  {label}: {} from {} to {} at {}, {} (visit {}, load {}, feature {}, type {})",
        arrival.estimated_arrival,
        arrival.origin_code,
        arrival.destination_code,
        arrival.latitude,
        arrival.longitude,
        arrival.visit_number,
        arrival.load,
        arrival.feature,
        arrival.type_
    );
}

#[cfg(test)]
mod tests {
    use super::generated::{GetBusArrivalResponse, decode_get_bus_arrival_response};

    #[test]
    fn decodes_unavailable_arrival_slots() {
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: br#"{"odata.metadata":"https://datamall2.mytransport.sg/ltaodataservice/v3/BusArrival","BusStopCode":"83139","Services":[{"ServiceNo":"15","Operator":"GAS","NextBus":{"OriginCode":"77009","DestinationCode":"77009","EstimatedArrival":"2026-06-02T23:48:03+08:00","Monitored":1,"Latitude":"1.3211655","Longitude":"103.9052625","VisitNumber":"1","Load":"SEA","Feature":"WAB","Type":"SD"},"NextBus2":{"OriginCode":"","DestinationCode":"","EstimatedArrival":"","Monitored":0,"Latitude":"","Longitude":"","VisitNumber":"","Load":"","Feature":"","Type":""},"NextBus3":{"OriginCode":"","DestinationCode":"","EstimatedArrival":"","Monitored":0,"Latitude":"","Longitude":"","VisitNumber":"","Load":"","Feature":"","Type":""}}]}"#
                .to_vec(),
        };

        let decoded = decode_get_bus_arrival_response(response).expect("decode response");
        let GetBusArrivalResponse::Ok(arrival) = decoded else {
            panic!("expected 200 response");
        };

        let service = arrival.services.first().expect("service");
        let next_bus = service.next_bus.as_ref().expect("next bus");
        assert_eq!(next_bus.origin_code, 77009);
        assert_eq!(next_bus.destination_code, 77009);
        assert_eq!(next_bus.latitude, 1.3211655);
        assert_eq!(next_bus.longitude, 103.9052625);
        assert_eq!(next_bus.visit_number, 1);
        assert!(service.next_bus2.is_none());
        assert!(service.next_bus3.is_none());
    }
}
