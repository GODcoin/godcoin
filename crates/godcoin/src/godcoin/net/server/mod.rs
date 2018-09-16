use super::rpc::codec::RpcCodec;

use std::io::{Error, ErrorKind};
use std::net::SocketAddr;

use tokio::net::TcpListener;
use tokio_codec::Framed;
use tokio::prelude::*;

use net::peer::*;
use net::rpc::*;

pub fn start(addr: SocketAddr) {
    let listener = TcpListener::bind(&addr).unwrap();
    info!("Server binded to {:?}", &addr);
    let server = listener.incoming().for_each(|socket| {
        let addr = socket.peer_addr().unwrap();
        info!("[{}] Accepted connection", addr);

        let frame = Framed::new(socket, RpcCodec::new());
        let hs = frame.into_future()
                        .map_err(|(e, _)| e)
                        .and_then(|(data, frame)| {
                            if let Some(data) = data { return Ok((data, frame)) }
                            Err(Error::from(ErrorKind::UnexpectedEof))
                        });
        let hs = hs.and_then(|(data, frame)| {
            if data.id != 0 {
                return Err(Error::new(ErrorKind::InvalidData, "id must be 0"))
            }
            if let Some(msg) = data.msg {
                let peer_type = match msg {
                    RpcMsg::Handshake(hs) => { hs.peer_type },
                    _ => return Err(Error::new(ErrorKind::InvalidData, "expected handshake msg"))
                };


                Ok((peer_type, frame))
            } else {
                Err(Error::new(ErrorKind::InvalidData, "expected handshake msg"))
            }
        });

        let client = hs.and_then(move |(peer_type, frame)| {
            let peer = Peer::new(peer_type, addr, frame);
            debug!("Handshake from client completed: {:?}", peer);
            peer.get_sender().send(RpcPayload {
                id: 0,
                msg: None
            });
            ::tokio::spawn(peer.for_each(move |msg| {
                info!("[{}] Received msg: {:?}", addr, msg);
                Ok(())
            }).and_then(move |_| {
                warn!("[{}] Client disconnected", addr);
                Ok(())
            }).map_err(move |e| {
                debug!("[{}] Error handling frame from client: {:?}", addr, e);
            }));

            Ok(())
        });

        ::tokio::spawn(client.map_err(move |e| {
            debug!("[{}] Handshake error: {:?}", addr, e);
        }));
        Ok(())
    }).map_err(|err| {
        error!("Server accept error: {:?}", err);
    });
    ::tokio::spawn(server);
}
