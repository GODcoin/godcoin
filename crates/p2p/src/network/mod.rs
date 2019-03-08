use super::server::Server;
use crate::*;
use std::collections::HashMap;

pub mod cmd;

struct Handlers<S: 'static> {
    connected: Option<Box<Fn(&mut S, SessionInfo) -> () + 'static>>,
    disconnected: Option<Box<Fn(&mut S, SessionInfo) -> () + 'static>>,
    message: Box<Fn(&mut S, SessionId, &Payload) -> bool + 'static>,
}

pub struct Network<S: 'static, M: 'static + Metrics = DummyMetrics> {
    state: S,
    handlers: Handlers<S>,
    metrics: M,
    sessions: HashMap<SessionId, (SessionInfo, Addr<Session>)>,
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
            metrics: DummyMetrics::default(),
            sessions: HashMap::with_capacity(32),
        }
    }
}

impl<S: 'static, M: 'static + Metrics> Network<S, M> {
    pub fn with_metrics<CM: 'static + Metrics>(self, metrics: CM) -> Network<S, CM> {
        Network {
            state: self.state,
            handlers: self.handlers,
            metrics,
            sessions: self.sessions,
        }
    }

    pub fn on_connect<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut S, SessionInfo) -> () + 'static,
    {
        self.handlers.connected.replace(Box::new(f));
        self
    }

    pub fn on_disconnect<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut S, SessionInfo) -> () + 'static,
    {
        self.handlers.disconnected.replace(Box::new(f));
        self
    }

    fn broadcast(&mut self, msg: &Payload, skip_id: Option<SessionId>) {
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

impl<S: 'static, M: 'static + Metrics> Handler<SessionMsg> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: SessionMsg, _: &mut Self::Context) {
        match msg {
            SessionMsg::Connected(ses, addr) => {
                let id = ses.id;
                let prev = self.sessions.insert(id, (ses.clone(), addr));
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
                    f(&mut self.state, ses.0);
                }
            }
            SessionMsg::Message(ses_id, payload) => {
                self.metrics.on_inbound_message(&payload);
                if (self.handlers.message)(&mut self.state, ses_id, &payload) {
                    self.broadcast(&payload, Some(ses_id));
                }
            }
        }
    }
}
