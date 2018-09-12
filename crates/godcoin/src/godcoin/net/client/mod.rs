use std::io::{Error, ErrorKind};
use tokio::net::TcpStream;
use std::net::SocketAddr;
use tokio_codec::Framed;
use tokio::prelude::*;
use rand;

use super::peer::*;
use super::rpc::*;

pub fn connect(addr: SocketAddr, client_type: ClientType) -> impl Future<Item = Peer, Error = Error> {
    TcpStream::connect(&addr).and_then(move |stream| {
        let frame = Framed::new(stream, codec::RpcCodec::new());
        let msg = RpcPayload {
            id: 0,
            msg: Some(RpcMsg::Handshake(RpcMsgHandshake {
                client_type
            }))
        };

        debug!("[{}] Sending handshake: {:?}", addr, &msg);
        frame.send(msg)
    }).and_then(move |frame| {
        let (resp, frame) = frame.into_future().map_err(|(e, _)| e).wait()?;
        let resp = resp.ok_or_else(|| Error::from(ErrorKind::UnexpectedEof))?;
        debug!("[{}] Received handshake message: {:?}", addr, &resp);
        if resp.id != 0 {
            return Err(Error::new(ErrorKind::InvalidData, "expected id to be 0"))
        } else if resp.msg.is_some() {
            return Err(Error::new(ErrorKind::InvalidData, "expected msg to be empty"))
        }
        Ok(Peer::new(client_type, addr, frame))
    })
}

pub fn backoff(tries: u8) -> u16 {
    let tries = f64::from(tries);
    let max = 15000f64;
    let rand = rand::random::<f64>();
    max.min((1.5f64.powf(tries) * 100f64 * rand).floor()) as u16
}

