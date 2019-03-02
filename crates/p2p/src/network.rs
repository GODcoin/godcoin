use super::server::Server;
use crate::*;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::{
    net::{TcpListener, TcpStream},
    prelude::*,
};

pub enum NetCmd {
    Listen(SocketAddr),
    Connect(SocketAddr),
}

impl Message for NetCmd {
    type Result = ();
}

#[derive(Debug)]
pub enum NetMsg {
    Connected(SessionInfo),
    Disconnected(SessionInfo),
    Message(SessionInfo, Payload),
}

impl Message for NetMsg {
    type Result = ();
}

pub struct Network {
    recipient: Recipient<NetMsg>,
    sessions: HashMap<SocketAddr, SessionInfo>,
}

impl Network {
    pub fn new(recipient: Recipient<NetMsg>) -> Self {
        Network {
            recipient,
            sessions: HashMap::with_capacity(32),
        }
    }

    fn broadcast(&self, msg: &Payload, skip: SocketAddr) {
        self.sessions
            .values()
            .filter(|ses| ses.addr != skip)
            .for_each(|ses| {
                let _ = ses.recipient.do_send(msg.clone());
            });
    }
}

impl Actor for Network {
    type Context = Context<Self>;
}

impl Handler<NetCmd> for Network {
    type Result = ();

    fn handle(&mut self, msg: NetCmd, ctx: &mut Self::Context) {
        match msg {
            NetCmd::Listen(bind_addr) => {
                let recipient = ctx.address().recipient();
                Server::create(move |ctx| {
                    let s = TcpListener::bind(&bind_addr).unwrap();
                    debug!("Accepting inbound connections on {}", bind_addr);
                    ctx.add_stream(s.incoming());
                    Server { recipient }
                });
            }
            NetCmd::Connect(addr) => {
                let rx = ctx.address().recipient();
                Arbiter::spawn(
                    TcpStream::connect(&addr)
                        .and_then(|s| {
                            Session::init(rx, ConnectionType::Outbound, s);
                            Ok(())
                        })
                        .map_err(move |e| {
                            warn!("[{}] Failed to connect to peer: {:?}", addr, e);
                        }),
                );
            }
        }
    }
}

impl Handler<SessionMsg> for Network {
    type Result = ();

    fn handle(&mut self, msg: SessionMsg, _: &mut Self::Context) {
        match msg {
            SessionMsg::Connected(ses) => {
                let addr = ses.addr;
                let prev = self.sessions.insert(addr, ses.clone());
                assert!(prev.is_none());
                self.recipient.do_send(NetMsg::Connected(ses)).unwrap();
            }
            SessionMsg::Disconnected(addr) => {
                let ses = self
                    .sessions
                    .remove(&addr)
                    .unwrap_or_else(|| panic!("Expected disconnected peer to exist: {}", addr));
                self.recipient.do_send(NetMsg::Disconnected(ses)).unwrap();
            }
            SessionMsg::Message(ses, payload) => {
                // TODO: ID caching to prevent broadcast loops
                self.broadcast(&payload, ses.addr);
                self.recipient
                    .do_send(NetMsg::Message(ses, payload))
                    .unwrap();
            }
        }
    }
}
