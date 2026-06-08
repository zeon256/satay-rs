export const HERO_RUST = `use generated::{Api, BusServiceNumber, GetBusArrivalResponse};
use satay_reqwest::{ReqwestActionExt, reqwest};

let api = Api::new().account_key(api_key);
let client = reqwest::Client::new();

let response = api
    .get_bus_arrival(83139)
    .service_no(BusServiceNumber::try_new("15")?)
    .send_with(&client)
    .await?;

match response {
    GetBusArrivalResponse::Ok(arrival) => println!("{arrival:?}"),
    GetBusArrivalResponse::UnexpectedStatus(status, body) => {
        eprintln!("unexpected status {status}");
    }
}`

export const GENERATE_SHELL = `$ cargo install satay-cli
$ satay generate --input openapi.yaml --output src/generated --rustfmt`
