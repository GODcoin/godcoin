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
    Disconnect(SessionId),
}

impl Message for NetCmd {
    type Result = ();
}

struct Handlers<S: 'static> {
    connected: Option<Box<Fn(&mut S, SessionInfo) -> () + 'static>>,
    disconnected: Option<Box<Fn(&mut S, SessionInfo) -> () + 'static>>,
    message: Box<Fn(&mut S, SessionId, Payload) -> () + 'static>,
}

pub struct Network<S: 'static> {
    state: S,
    handlers: Handlers<S>,
    sessions: HashMap<SessionId, SessionInfo>,
}

impl<S: 'static> Network<S> {
    pub fn new<F>(state: S, msg_handler: F) -> Self
    where
        F: Fn(&mut S, SessionId, Payload) -> () + 'static,
    {
        let handlers = Handlers {
            connected: None,
            disconnected: None,
            message: Box::new(msg_handler),
        };
        Network {
            state,
            handlers,
            sessions: HashMap::with_capacity(32),
        }
    }

    pub fn on_connect<F>(mut self, f: F) -> Network<S>
    where
        F: Fn(&mut S, SessionInfo) -> () + 'static,
    {
        self.handlers.connected.replace(Box::new(f));
        self
    }

    pub fn on_disconnect<F>(mut self, f: F) -> Network<S>
    where
        F: Fn(&mut S, SessionInfo) -> () + 'static,
    {
        self.handlers.disconnected.replace(Box::new(f));
        self
    }

    fn broadcast(&self, msg: &Payload, skip: SessionId) {
        self.sessions
            .values()
            .filter(|ses| ses.id != skip)
            .for_each(|ses| {
                ses.address.do_send(msg.clone());
            });
    }
}

impl<S: 'static> Actor for Network<S> {
    type Context = Context<Self>;
}

impl<S: 'static> Handler<NetCmd> for Network<S> {
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
            NetCmd::Disconnect(id) => {
                if let Some(ses) = self.sessions.get(&id) {
                    ses.address.do_send(session::Disconnect);
                }
            }
        }
    }
}

impl<S: 'static> Handler<SessionMsg> for Network<S> {
    type Result = ();

    fn handle(&mut self, msg: SessionMsg, _: &mut Self::Context) {
        match msg {
            SessionMsg::Connected(ses) => {
                let id = ses.id;
                let prev = self.sessions.insert(id, ses.clone());
                assert!(prev.is_none());
                if let Some(f) = &self.handlers.connected {
                    f(&mut self.state, ses);
                }
            }
            SessionMsg::Disconnected(addr) => {
                let ses = self
                    .sessions
                    .remove(&addr)
                    .unwrap_or_else(|| panic!("Expected disconnected peer to exist: {}", addr));
                if let Some(f) = &self.handlers.disconnected {
                    f(&mut self.state, ses);
                }
            }
            SessionMsg::Message(ses_id, payload) => {
                // TODO: ID caching to prevent broadcast loops
                self.broadcast(&payload, ses_id);
                (self.handlers.message)(&mut self.state, ses_id, payload);
            }
        }
    }
}
