mod codec;
mod rpc;

use self::codec::RpcCodec;

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tokio_codec::Framed;
use tokio::prelude::*;

pub fn start_server(addr: &SocketAddr) {
    let listener = TcpListener::bind(&addr).unwrap();
    println!("Server binded to {:?}", &addr);
    let server = listener.incoming().for_each(|socket| {
        let addr = socket.peer_addr().unwrap();
        println!("[{:?}] Accepted connection", &addr);

        let framed_sock = Framed::new(socket, RpcCodec::new());
        ::tokio::spawn(framed_sock.for_each(|rpc| {
            // TODO: process messages
            println!("Received payload {:?}", rpc);
            Ok(())
        }).and_then(move |_| {
            println!("[{:?}] Connection closed", addr);
            Ok(())
        }).map_err(move |err| {
            println!("[{:?}] Connection closed with error: {:?}", addr, err);
        }));

        Ok(())
    }).map_err(|err| {
        println!("Server accept error: {:?}", err);
    });
    ::tokio::spawn(server);
}
