use std::future;
use std::net::SocketAddr;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::error::Error;
use crate::wire::{WireRequest, WireResponse};

pub(crate) struct TungsteniteTransport {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next_id: u64,
}

impl TungsteniteTransport {
    pub(crate) async fn connect(addr: SocketAddr) -> Result<Self, Error> {
        let (socket, _) = connect_async(format!("ws://{addr}/rpc")).await?;
        Ok(Self { socket, next_id: 1 })
    }

    pub(crate) async fn close(&mut self) -> Result<(), Error> {
        Ok(self.socket.close(None).await?)
    }

    async fn round_trip(
        &mut self,
        request: http::Request<Vec<u8>>,
    ) -> Result<satay_runtime::ResponseParts<Vec<u8>>, Error> {
        let id = self.next_id;
        self.next_id += 1;

        let wire_request = WireRequest::from_http(id, request);
        let message = serde_json::to_string(&wire_request)?;
        self.socket.send(Message::Text(message.into())).await?;

        loop {
            let Some(message) = self.socket.next().await else {
                return Err(Error::Closed);
            };
            let message = message?;

            let wire_response = match message {
                Message::Text(text) => serde_json::from_str::<WireResponse>(text.as_str())?,
                Message::Binary(bytes) => serde_json::from_slice::<WireResponse>(&bytes)?,
                Message::Close(_) => return Err(Error::Closed),
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
            };

            if wire_response.id == id {
                return wire_response.into_response_parts();
            }
        }
    }
}

pub(crate) trait TungsteniteActionExt: satay_runtime::Action + Sized {
    fn send_over_ws<'a>(
        self,
        transport: &'a mut TungsteniteTransport,
    ) -> impl future::Future<Output = Result<Self::OwnedResponse, Error>> + 'a
    where
        Self: satay_runtime::OwnedAction + 'a,
        Self::RequestBody: Into<Vec<u8>>,
    {
        async move {
            let request = self.request()?.map(Into::into);
            let response = transport.round_trip(request).await?;
            Ok(satay_runtime::BufferedResponse::<Self, _>::new(response).decode_owned()?)
        }
    }
}

impl<T: satay_runtime::Action> TungsteniteActionExt for T {}
