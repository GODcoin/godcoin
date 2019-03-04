use crate::*;
use std::net::SocketAddr;
use std::{fmt, io::Error};
use tokio::{codec::FramedRead, io::WriteHalf, net::TcpStream, prelude::*};

pub type SessionId = SocketAddr;

#[derive(Message)]
pub enum SessionMsg {
    Connected(SessionInfo),
    Disconnected(SocketAddr),
    Message(SessionId, Payload),
}

#[derive(Message)]
pub struct Disconnect;

#[derive(Clone, Debug)]
pub enum ConnectionType {
    Inbound,
    Outbound,
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConnectionType::Inbound => f.write_str("inbound"),
            ConnectionType::Outbound => f.write_str("outbound"),
        }
    }
}

#[derive(Clone)]
pub struct SessionInfo {
    pub id: SocketAddr,
    pub conn_type: ConnectionType,
    pub addr: SocketAddr,
    pub ses_addr: Addr<Session>,
}

impl std::fmt::Debug for SessionInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SessionInfo")
            .field("conn_type", &self.conn_type)
            .field("addr", &self.addr)
            .finish()
    }
}

pub struct Session {
    recipient: Recipient<SessionMsg>,
    write: actix::io::FramedWrite<WriteHalf<TcpStream>, Codec>,
    info: SessionInfo,
}

impl Session {
    pub fn init(server_rx: Recipient<SessionMsg>, conn_type: ConnectionType, stream: TcpStream) {
        // TODO: perform the handshake
        let addr = stream.peer_addr().unwrap();
        debug!("[{}] Accepted {} connection", addr, conn_type);
        Session::create(move |ctx| {
            let (r, w) = stream.split();
            ctx.add_stream(FramedRead::new(r, Codec::new()));
            Session {
                recipient: server_rx,
                write: actix::io::FramedWrite::new(w, Codec::new(), ctx),
                info: SessionInfo {
                    id: addr,
                    conn_type,
                    addr,
                    ses_addr: ctx.address(),
                },
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

impl actix::io::WriteHandler<Error> for Session {}

impl Handler<Disconnect> for Session {
    type Result = ();

    fn handle(&mut self, _: Disconnect, ctx: &mut Self::Context) {
        ctx.stop();
    }
}

impl Handler<Payload> for Session {
    type Result = ();

    fn handle(&mut self, msg: Payload, _: &mut Self::Context) {
        debug!("[{}] Sent payload: {:?}", self.info.addr, &msg);
        self.write.write(msg);
    }
}

impl StreamHandler<Payload, Error> for Session {
    fn handle(&mut self, msg: Payload, _: &mut Self::Context) {
        debug!("[{}] Received payload: {:?}", self.info.addr, msg);
        self.recipient
            .do_send(SessionMsg::Message(self.info.id, msg))
            .unwrap();
    }

    fn error(&mut self, err: Error, _: &mut Self::Context) -> Running {
        error!("[{}] Frame handling error: {:?}", self.info.addr, err);
        Running::Stop
    }
}
