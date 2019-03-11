use crate::*;
use std::net::SocketAddr;
use std::{fmt, io::Error};
use tokio::{io::WriteHalf, net::TcpStream};

pub type SessionId = SocketAddr;

#[derive(Message)]
pub enum SessionMsg {
    Connected(SessionInfo, Addr<Session>),
    Disconnected(SocketAddr),
    Message(SessionId, Payload),
}

impl fmt::Debug for SessionMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SessionMsg::Connected(info, _) => f.debug_tuple("Connected").field(&info).finish(),
            SessionMsg::Disconnected(addr) => f.debug_tuple("Disconnected").field(&addr).finish(),
            SessionMsg::Message(id, payload) => {
                f.debug_tuple("Message").field(&id).field(&payload).finish()
            }
        }
    }
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

#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub id: SocketAddr,
    pub conn_type: ConnectionType,
    pub peer_addr: SocketAddr,
}

pub struct Session {
    pub recipient: Recipient<SessionMsg>,
    pub writer: actix::io::FramedWrite<WriteHalf<TcpStream>, Codec>,
    pub info: SessionInfo,
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
        self.writer.write(msg);
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
