use std::net::SocketAddr;

use futures_util::{SinkExt, StreamExt};
use http::header::CONTENT_TYPE;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use crate::error::Error;
use crate::wire::{WireHeader, WireRequest, WireResponse};

pub(crate) async fn spawn_local_server() -> Result<SocketAddr, Error> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };

            tokio::spawn(async move {
                if let Err(error) = handle_connection(stream).await {
                    eprintln!("websocket server error: {error}");
                }
            });
        }
    });

    Ok(addr)
}

async fn handle_connection(stream: TcpStream) -> Result<(), Error> {
    let mut socket = accept_async(stream).await?;

    while let Some(message) = socket.next().await {
        let message = message?;
        let wire_request = match message {
            Message::Text(text) => serde_json::from_str::<WireRequest>(text.as_str())?,
            Message::Binary(bytes) => serde_json::from_slice::<WireRequest>(&bytes)?,
            Message::Close(_) => return Ok(()),
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
        };

        println!(
            "server received {} {}",
            wire_request.method, wire_request.uri
        );

        let wire_response = route_request(wire_request);
        let message = serde_json::to_string(&wire_response)?;
        socket.send(Message::Text(message.into())).await?;
    }

    Ok(())
}

fn route_request(request: WireRequest) -> WireResponse {
    if request.method == "GET" && request.uri.contains("/BusArrival") {
        WireResponse {
            id: request.id,
            status: 200,
            headers: vec![WireHeader {
                name: CONTENT_TYPE.as_str().to_owned(),
                value: b"application/json".to_vec(),
            }],
            body: BUS_ARRIVAL_RESPONSE.as_bytes().to_vec(),
        }
    } else {
        WireResponse {
            id: request.id,
            status: 404,
            headers: vec![WireHeader {
                name: CONTENT_TYPE.as_str().to_owned(),
                value: b"text/plain".to_vec(),
            }],
            body: b"not found".to_vec(),
        }
    }
}

const BUS_ARRIVAL_RESPONSE: &str = r#"{
  "odata.metadata": "https://datamall2.mytransport.sg/ltaodataservice/v3/BusArrival",
  "BusStopCode": "83139",
  "Services": [
    {
      "ServiceNo": "15",
      "Operator": "GAS",
      "NextBus": {
        "OriginCode": "77009",
        "DestinationCode": "77131",
        "EstimatedArrival": "2024-08-14T16:41:48+08:00",
        "Latitude": "1.3154918333333334",
        "Longitude": "103.9059125",
        "VisitNumber": "1",
        "Load": "SEA",
        "Feature": "WAB",
        "Type": "SD"
      },
      "NextBus2": {
        "OriginCode": "77009",
        "DestinationCode": "77131",
        "EstimatedArrival": "2024-08-14T16:49:22+08:00",
        "Latitude": "1.3309621666666667",
        "Longitude": "103.9034135",
        "VisitNumber": "1",
        "Load": "SEA",
        "Feature": "WAB",
        "Type": "SD"
      },
      "NextBus3": {
        "OriginCode": "77009",
        "DestinationCode": "77131",
        "EstimatedArrival": "2024-08-14T17:06:11+08:00",
        "Latitude": "1.344761",
        "Longitude": "103.94022316666667",
        "VisitNumber": "1",
        "Load": "SEA",
        "Feature": "WAB",
        "Type": "SD"
      }
    }
  ]
}"#;
