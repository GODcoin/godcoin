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

impl<S: 'static, M: 'static + Metrics> Handler<cmd::Listen> for Network<S, M> {
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

impl<S: 'static, M: 'static + Metrics> Handler<cmd::Connect> for Network<S, M> {
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

impl<S: 'static, M: 'static + Metrics> Handler<cmd::Disconnect> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: cmd::Disconnect, _: &mut Self::Context) {
        if let Some(ses) = self.sessions.get(&msg.0) {
            ses.1.do_send(session::cmd::Disconnect);
        }
    }
}

impl<S: 'static, M: 'static + Metrics> Handler<cmd::Broadcast> for Network<S, M> {
    type Result = ();

    fn handle(&mut self, msg: cmd::Broadcast, _: &mut Self::Context) {
        self.broadcast(&msg.0, None);
    }
}

impl<S: 'static, M: 'static + Metrics> Handler<cmd::Metrics<M>> for Network<S, M>
where
    M: actix::dev::MessageResponse<Network<S, M>, cmd::Metrics<M>>,
{
    type Result = M;

    fn handle(&mut self, _: cmd::Metrics<M>, _: &mut Self::Context) -> Self::Result {
        self.metrics.clone()
    }
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

    #[derive(Default, Message)]
    #[rtype(result = "M")]
    pub struct Metrics<M: 'static + crate::Metrics> {
        _metrics: std::marker::PhantomData<M>,
    }
}
