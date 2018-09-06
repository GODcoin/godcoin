use super::codec::RpcCodec;

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tokio_codec::Framed;
use tokio::prelude::*;

pub fn start(addr: &SocketAddr) {
    let listener = TcpListener::bind(&addr).unwrap();
    info!("Server binded to {:?}", &addr);
    let server = listener.incoming().for_each(|socket| {
        let addr = socket.peer_addr().unwrap();
        info!("[{:?}] Accepted connection", &addr);

        let framed_sock = Framed::new(socket, RpcCodec::new());
        ::tokio::spawn(framed_sock.for_each(|rpc| {
            // TODO: process messages
            info!("Received payload {:?}", rpc);
            Ok(())
        }).and_then(move |_| {
            info!("[{:?}] Connection closed", addr);
            Ok(())
        }).map_err(move |err| {
            warn!("[{:?}] Connection closed with error: {:?}", addr, err);
        }));

        Ok(())
    }).map_err(|err| {
        error!("Server accept error: {:?}", err);
    });
    ::tokio::spawn(server);
}
