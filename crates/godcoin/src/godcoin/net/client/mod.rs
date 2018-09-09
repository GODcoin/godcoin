use tokio::net::TcpStream;
use std::net::SocketAddr;
use tokio_codec::Framed;
use tokio::prelude::*;
use std::io::Error;

use super::peer::*;
use super::rpc::*;

pub fn connect(addr: SocketAddr, client_type: ClientType) -> impl Future<Item = Peer, Error = Error> {
    let tcp = TcpStream::connect(&addr);

    let handshake = tcp.and_then(move |stream| {
        let hs = Framed::new(stream, codec::RpcCodec::new());
        let msg = RpcPayload {
            id: 0,
            msg: Some(RpcMsg::Handshake(RpcMsgHandshake {
                client_type
            }))
        };

        info!("Connected to server {}", addr);
        hs.send(msg).map(|hs| hs.into_inner())
    });

    handshake.and_then(move |stream| {
        let frame = Framed::new(stream, codec::RpcCodec::new());
        Ok(Peer::new(client_type, addr, frame))
    })
}
