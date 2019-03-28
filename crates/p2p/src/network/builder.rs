use super::*;

struct BuilderHandlers<S: 'static, M: 'static + Metrics> {
    connected: Option<Box<Fn(&Addr<Network<S, M>>, &mut S, PeerInfo) -> () + 'static>>,
    disconnected: Option<Box<Fn(&Addr<Network<S, M>>, &mut S, PeerInfo) -> () + 'static>>,
    connect_req: Option<
        Box<
            Fn(&Addr<Network<S, M>>, &mut S, PeerInfo, Payload) -> peer::msg::HandshakeRequest
                + 'static,
        >,
    >,
    message: Option<Box<Fn(&Addr<Network<S, M>>, &mut S, PeerId, &Payload) -> bool + 'static>>,
}

impl<S: 'static, M: 'static + Metrics> Default for BuilderHandlers<S, M> {
    fn default() -> Self {
        BuilderHandlers {
            connected: None,
            disconnected: None,
            connect_req: None,
            message: None,
        }
    }
}

pub struct Builder<S: 'static, M: 'static + Metrics = DummyMetrics> {
    state: S,
    metrics: M,
    handlers: BuilderHandlers<S, M>,
}

impl<S: 'static> Builder<S> {
    pub fn new(state: S) -> Self {
        Builder {
            state,
            metrics: DummyMetrics::default(),
            handlers: BuilderHandlers::default(),
        }
    }
}

impl<S: 'static, M: 'static + Metrics> Builder<S, M> {
    pub fn start(self) -> Addr<Network<S, M>> {
        let handlers = Handlers {
            connected: self.handlers.connected,
            disconnected: self.handlers.disconnected,
            connect_req: self.handlers.connect_req,
            message: self
                .handlers
                .message
                .expect("expected message handler to init network"),
        };
        Network::start(self.state, self.metrics, handlers)
    }

    pub fn with_metrics(state: S, metrics: M) -> Self {
        Builder {
            state,
            metrics,
            handlers: BuilderHandlers::default(),
        }
    }

    pub fn on_message<F>(mut self, f: F) -> Self
    where
        F: Fn(&Addr<Network<S, M>>, &mut S, PeerId, &Payload) -> bool + 'static,
    {
        self.handlers.message.replace(Box::new(f));
        self
    }

    pub fn on_connect_req<F>(mut self, f: F) -> Self
    where
        F: Fn(&Addr<Network<S, M>>, &mut S, PeerInfo, Payload) -> peer::msg::HandshakeRequest
            + 'static,
    {
        self.handlers.connect_req.replace(Box::new(f));
        self
    }

    pub fn on_connect<F>(mut self, f: F) -> Self
    where
        F: Fn(&Addr<Network<S, M>>, &mut S, PeerInfo) -> () + 'static,
    {
        self.handlers.connected.replace(Box::new(f));
        self
    }

    pub fn on_disconnect<F>(mut self, f: F) -> Self
    where
        F: Fn(&Addr<Network<S, M>>, &mut S, PeerInfo) -> () + 'static,
    {
        self.handlers.disconnected.replace(Box::new(f));
        self
    }
}
