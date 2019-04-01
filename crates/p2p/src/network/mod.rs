use super::server::Server;
use crate::*;
use std::collections::HashMap;

pub mod builder;
pub mod cmd;
pub mod connect;

pub use builder::Builder;

pub struct Handlers<S: 'static, M: 'static + Metrics> {
    connected: Option<Box<Fn(&Addr<Network<S, M>>, &mut S, PeerInfo) -> () + 'static>>,
    disconnected: Option<Box<Fn(&Addr<Network<S, M>>, &mut S, PeerInfo) -> () + 'static>>,
    connect_req: Option<
        Box<
            Fn(&Addr<Network<S, M>>, &mut S, PeerInfo, Payload) -> peer::msg::HandshakeRequest
                + 'static,
        >,
    >,
    message: Box<Fn(&Addr<Network<S, M>>, &mut S, PeerId, &Payload) -> bool + 'static>,
}

pub struct Network<S: 'static, M: 'static + Metrics = DummyMetrics> {
    addr: Addr<Self>,
    state: S,
    handlers: Handlers<S, M>,
    metrics: M,
    sessions: HashMap<PeerId, (PeerInfo, Addr<Peer<S, M>>)>,
}

impl<S: 'static, M: 'static + Metrics> Network<S, M> {
    pub fn start(state: S, metrics: M, handlers: Handlers<S, M>) -> Addr<Network<S, M>> {
        Network::create(|ctx| Network {
            addr: ctx.address(),
            state,
            handlers,
            metrics,
            sessions: HashMap::with_capacity(32),
        })
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

impl<S: 'static, M: 'static + Metrics> Handler<peer::msg::Handshake> for Network<S, M> {
    type Result = peer::msg::HandshakeRequest;

    fn handle(&mut self, msg: peer::msg::Handshake, _: &mut Self::Context) -> Self::Result {
        if let Some(f) = &self.handlers.connect_req {
            f(&self.addr, &mut self.state, msg.0, msg.1)
        } else {
            peer::msg::HandshakeRequest::Allow
        }
    }
}

impl<S: 'static, M: 'static + Metrics> Handler<peer::msg::Connected<S, M>> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: peer::msg::Connected<S, M>, _: &mut Self::Context) {
        let info = msg.0;
        let prev = self.sessions.insert(info.id, (info.clone(), msg.1));
        assert!(prev.is_none(), "session id already exists in network");
        if let Some(f) = &self.handlers.connected {
            f(&self.addr, &mut self.state, info);
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
            f(&self.addr, &mut self.state, ses.0);
        }
    }
}

impl<S: 'static, M: 'static + Metrics> Handler<peer::msg::Message> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: peer::msg::Message, _: &mut Self::Context) {
        let id = msg.0;
        let pl = &msg.1;
        self.metrics.on_inbound_message(pl);
        if (self.handlers.message)(&self.addr, &mut self.state, id, pl) {
            self.broadcast(pl, Some(id));
        }
    }
}
