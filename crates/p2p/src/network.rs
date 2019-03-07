use super::server::Server;
use crate::*;
use std::collections::HashMap;
use tokio::{
    net::{TcpListener, TcpStream},
    prelude::*,
};

struct Handlers<S: 'static> {
    connected: Option<Box<Fn(&mut S, SessionInfo) -> () + 'static>>,
    disconnected: Option<Box<Fn(&mut S, SessionInfo) -> () + 'static>>,
    message: Box<Fn(&mut S, SessionId, &Payload) -> bool + 'static>,
}

pub struct Network<S: 'static> {
    state: S,
    handlers: Handlers<S>,
    sessions: HashMap<SessionId, SessionInfo>,
}

impl<S: 'static> Network<S> {
    pub fn new<F>(state: S, msg_handler: F) -> Self
    where
        F: Fn(&mut S, SessionId, &Payload) -> bool + 'static,
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

    fn broadcast(&self, msg: &Payload, skip_id: Option<SessionId>) {
        self.sessions
            .values()
            .filter(|ses| skip_id.map_or(true, |skip_id| ses.id != skip_id))
            .for_each(|ses| {
                ses.address.do_send(msg.clone());
            });
    }
}

impl<S: 'static> Actor for Network<S> {
    type Context = Context<Self>;
}

impl<S: 'static> Handler<cmd::Listen> for Network<S> {
    type Result = ();

    fn handle(&mut self, msg: cmd::Listen, ctx: &mut Self::Context) {
        let bind_addr = msg.0;
        let recipient = ctx.address().recipient();
        Server::create(move |ctx| {
            let s = TcpListener::bind(&bind_addr).unwrap();
            debug!("Accepting inbound connections on {}", bind_addr);
            ctx.add_stream(s.incoming());
            Server { recipient }
        });
    }
}

impl<S: 'static> Handler<cmd::Connect> for Network<S> {
    type Result = ();

    fn handle(&mut self, msg: cmd::Connect, ctx: &mut Self::Context) {
        let addr = msg.0;
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

impl<S: 'static> Handler<cmd::Disconnect> for Network<S> {
    type Result = ();

    fn handle(&mut self, msg: cmd::Disconnect, _: &mut Self::Context) {
        if let Some(ses) = self.sessions.get(&msg.0) {
            ses.address.do_send(session::Disconnect);
        }
    }
}

impl<S: 'static> Handler<cmd::Broadcast> for Network<S> {
    type Result = ();

    fn handle(&mut self, msg: cmd::Broadcast, _: &mut Self::Context) {
        self.broadcast(&msg.0, None);
    }
}

impl<S: 'static> Handler<SessionMsg> for Network<S> {
    type Result = ();

    fn handle(&mut self, msg: SessionMsg, _: &mut Self::Context) {
        match msg {
            SessionMsg::Connected(ses) => {
                let id = ses.id;
                let prev = self.sessions.insert(id, ses.clone());
                assert!(prev.is_none(), "session id already exists in network");
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
                if (self.handlers.message)(&mut self.state, ses_id, &payload) {
                    self.broadcast(&payload, Some(ses_id));
                }
            }
        }
    }
}

pub mod cmd {
    use super::*;
    use std::net::SocketAddr;

    #[derive(Message)]
    pub struct Listen(pub SocketAddr);

    #[derive(Message)]
    pub struct Connect(pub SocketAddr);

    #[derive(Message)]
    pub struct Disconnect(pub SessionId);

    #[derive(Message)]
    pub struct Broadcast(pub Payload);
}
