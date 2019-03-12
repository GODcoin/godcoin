use super::server::Server;
use crate::*;
use std::collections::HashMap;

pub mod cmd;
pub mod connect;

pub use connect::ConnectionType;

struct Handlers<S: 'static> {
    connected: Option<Box<Fn(&mut S, PeerInfo) -> () + 'static>>,
    disconnected: Option<Box<Fn(&mut S, PeerInfo) -> () + 'static>>,
    connect_req: Option<Box<Fn(&mut S, &PeerInfo, Payload) -> bool + 'static>>,
    message: Box<Fn(&mut S, PeerId, &Payload) -> bool + 'static>,
}

pub struct Network<S: 'static, M: 'static + Metrics = DummyMetrics> {
    state: S,
    handlers: Handlers<S>,
    metrics: M,
    sessions: HashMap<PeerId, (PeerInfo, Addr<Peer<S, M>>)>,
}

impl<S: 'static> Network<S> {
    pub fn new<F>(state: S, msg_handler: F) -> Self
    where
        F: Fn(&mut S, PeerId, &Payload) -> bool + 'static,
    {
        let handlers = Handlers {
            connected: None,
            disconnected: None,
            connect_req: None,
            message: Box::new(msg_handler),
        };
        Network {
            state,
            handlers,
            metrics: DummyMetrics::default(),
            sessions: HashMap::with_capacity(32),
        }
    }
}

impl<S: 'static, M: 'static + Metrics> Network<S, M> {
    pub fn with_metrics<CM: 'static + Metrics>(self, metrics: CM) -> Network<S, CM> {
        assert!(self.sessions.is_empty(), "sessions must be empty");
        Network {
            state: self.state,
            handlers: self.handlers,
            metrics,
            sessions: HashMap::with_capacity(32),
        }
    }

    pub fn on_connect_req<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut S, &PeerInfo, Payload) -> bool + 'static,
    {
        self.handlers.connect_req.replace(Box::new(f));
        self
    }

    pub fn on_connect<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut S, PeerInfo) -> () + 'static,
    {
        self.handlers.connected.replace(Box::new(f));
        self
    }

    pub fn on_disconnect<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut S, PeerInfo) -> () + 'static,
    {
        self.handlers.disconnected.replace(Box::new(f));
        self
    }

    fn broadcast(&mut self, msg: &Payload, skip_id: Option<PeerId>) {
        for ses in self
            .sessions
            .values()
            .filter(|ses| skip_id.map_or(true, |skip_id| ses.0.id != skip_id))
        {
            self.metrics.on_outbound_message(&msg);
            ses.1.do_send(msg.clone());
        }
    }
}

impl<S: 'static, M: 'static + Metrics> Actor for Network<S, M> {
    type Context = Context<Self>;
}

impl<S: 'static, M: 'static + Metrics> Handler<peer::msg::Connected<S, M>> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: peer::msg::Connected<S, M>, _: &mut Self::Context) {
        let info = msg.0;
        let prev = self.sessions.insert(info.id, (info.clone(), msg.1));
        assert!(prev.is_none(), "session id already exists in network");
        if let Some(f) = &self.handlers.connected {
            f(&mut self.state, info);
        }
    }
}

impl<S: 'static, M: 'static + Metrics> Handler<peer::msg::Disconnected> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: peer::msg::Disconnected, _: &mut Self::Context) {
        let id = msg.0.id;
        let ses = self
            .sessions
            .remove(&id)
            .unwrap_or_else(|| panic!("Expected disconnected peer to exist: {}", id));
        if let Some(f) = &self.handlers.disconnected {
            f(&mut self.state, ses.0);
        }
    }
}

impl<S: 'static, M: 'static + Metrics> Handler<peer::msg::Message> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: peer::msg::Message, _: &mut Self::Context) {
        let id = msg.0;
        let pl = &msg.1;
        self.metrics.on_inbound_message(pl);
        if (self.handlers.message)(&mut self.state, id, pl) {
            self.broadcast(pl, Some(id));
        }
    }
}
