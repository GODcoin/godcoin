use tokio::{
    net::TcpStream,
    codec::Framed,
    prelude::*
};
use std::net::SocketAddr;
use std::{
    io::Error,
    fmt
};
use crate::*;

#[derive(Message)]
pub enum SessionMsg {
    Connected(SessionInfo),
    Disconnected(SocketAddr)
}

#[derive(Clone, Debug)]
pub enum ConnectionType {
    Inbound,
    Outbound
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConnectionType::Inbound => f.write_str("inbound"),
            ConnectionType::Outbound => f.write_str("outbound")
        }
    }
}

#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub conn_type: ConnectionType,
    pub addr: SocketAddr
}

pub struct Session {
    recipient: Recipient<SessionMsg>,
    info: SessionInfo
}

impl Session {
    pub fn init(server_rx: Recipient<SessionMsg>,
                conn_type: ConnectionType,
                stream: TcpStream) {
        // TODO: perform the handshake
        let addr = stream.peer_addr().unwrap();
        debug!("[{}] Accepted {} connection", addr, conn_type);
        Session::create(move |ctx| {
            let (_, rx) = Framed::new(stream, Codec::new()).split();
            ctx.add_stream(rx);
            Session {
                recipient: server_rx,
                info: SessionInfo {
                    conn_type,
                    addr
                }
            }
        });
    }
}

impl Actor for Session {
    type Context = Context<Self>;

    fn started(&mut self, _: &mut Self::Context) {
        self.recipient
            .do_send(SessionMsg::Connected(self.info.clone()))
            .unwrap();
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        self.recipient
            .do_send(SessionMsg::Disconnected(self.info.addr))
            .unwrap();
    }
}

impl StreamHandler<Payload, Error> for Session {
    fn handle(&mut self, msg: Payload, _: &mut Self::Context) {
        debug!("[{}] Received frame: {:?}", self.info.addr, msg);
    }

    fn error(&mut self, err: Error, _: &mut Self::Context) -> Running {
        error!("[{}] Frame handling error: {:?}", self.info.addr, err);
        Running::Stop
    }
}
