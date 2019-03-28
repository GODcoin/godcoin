use crate::*;
use std::net::SocketAddr;
use std::{fmt, io::Error};
use tokio::{io::WriteHalf, net::TcpStream};

#[derive(Message)]
pub enum SessionMsg {
    Connected(Addr<Session>, SocketAddr),
    Disconnected(String),
    Message(Payload),
}

impl fmt::Debug for SessionMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SessionMsg::Connected(_, addr) => f.debug_tuple("Connected").field(&addr).finish(),
            SessionMsg::Disconnected(reason) => {
                f.debug_tuple("Disconnected").field(&reason).finish()
            }
            SessionMsg::Message(msg) => f.debug_tuple("Message").field(&msg).finish(),
        }
    }
}

pub struct Session {
    pub recipient: Recipient<SessionMsg>,
    pub writer: actix::io::FramedWrite<WriteHalf<TcpStream>, Codec>,
    pub peer_addr: SocketAddr,
    pub disconnect_reason: String,
}

impl Actor for Session {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.recipient
            .do_send(SessionMsg::Connected(ctx.address(), self.peer_addr))
            .unwrap();
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        let dc_reason = self.disconnect_reason.clone();
        let _ = self.recipient.do_send(SessionMsg::Disconnected(dc_reason));
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
        debug!("[{}] Sent payload: {:?}", self.peer_addr, &msg);
        self.writer.write(ProtocolMsg::Payload(msg));
    }
}

impl StreamHandler<ProtocolMsg, Error> for Session {
    fn handle(&mut self, msg: ProtocolMsg, ctx: &mut Self::Context) {
        debug!("[{}] Received payload: {:?}", self.peer_addr, msg);
        match msg {
            ProtocolMsg::Payload(msg) => {
                self.recipient.do_send(SessionMsg::Message(msg)).unwrap();
            }
            ProtocolMsg::Disconnect(reason) => {
                self.disconnect_reason = reason;
                ctx.stop();
            }
        }
    }

    fn error(&mut self, err: Error, _: &mut Self::Context) -> Running {
        error!("[{}] Frame handling error: {:?}", self.peer_addr, err);
        Running::Stop
    }
}

pub mod cmd {
    use super::*;

    #[derive(Message)]
    pub struct Disconnect(pub String);
}
