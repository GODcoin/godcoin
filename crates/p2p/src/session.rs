use crate::*;
use std::net::SocketAddr;
use std::{fmt, io::Error};
use tokio::{codec::FramedRead, io::WriteHalf, net::TcpStream, prelude::*};

pub type SessionId = SocketAddr;

#[derive(Message)]
pub enum SessionMsg {
    Connected(SessionInfo, Addr<Session>),
    Disconnected(SocketAddr),
    Message(SessionId, Payload),
}

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
    pub peer_addr: SocketAddr,
}

impl std::fmt::Debug for SessionInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SessionInfo")
            .field("conn_type", &self.conn_type)
            .field("peer_addr", &self.peer_addr)
            .finish()
    }
}

pub struct Session {
    recipient: Recipient<SessionMsg>,
    write: actix::io::FramedWrite<WriteHalf<TcpStream>, Codec>,
    info: SessionInfo,
}

impl Session {
    pub fn init(tx: Recipient<SessionMsg>, conn_type: ConnectionType, stream: TcpStream) {
        // TODO: perform the handshake
        let peer_addr = stream.peer_addr().unwrap();
        debug!("[{}] Accepted {} connection", peer_addr, conn_type);
        Session::create(move |ctx| {
            let (r, w) = stream.split();
            ctx.add_stream(FramedRead::new(r, Codec::new()));
            Session {
                recipient: tx,
                write: actix::io::FramedWrite::new(w, Codec::new(), ctx),
                info: SessionInfo {
                    id: peer_addr,
                    conn_type,
                    peer_addr,
                },
            }
        });
    }
}

impl Actor for Session {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.recipient
            .do_send(SessionMsg::Connected(self.info.clone(), ctx.address()))
            .unwrap();
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        self.recipient
            .do_send(SessionMsg::Disconnected(self.info.id))
            .unwrap();
    }
}

impl actix::io::WriteHandler<Error> for Session {}

impl Handler<cmd::Disconnect> for Session {
    type Result = ();

    fn handle(&mut self, _: cmd::Disconnect, ctx: &mut Self::Context) {
        ctx.stop();
    }
}

impl Handler<Payload> for Session {
    type Result = ();

    fn handle(&mut self, msg: Payload, _: &mut Self::Context) {
        debug!("[{}] Sent payload: {:?}", self.info.id, &msg);
        self.write.write(msg);
    }
}

impl StreamHandler<Payload, Error> for Session {
    fn handle(&mut self, msg: Payload, _: &mut Self::Context) {
        debug!("[{}] Received payload: {:?}", self.info.id, msg);
        self.recipient
            .do_send(SessionMsg::Message(self.info.id, msg))
            .unwrap();
    }

    fn error(&mut self, err: Error, _: &mut Self::Context) -> Running {
        error!("[{}] Frame handling error: {:?}", self.info.id, err);
        Running::Stop
    }
}

pub mod cmd {
    use super::*;

    #[derive(Message)]
    pub struct Disconnect;
}
