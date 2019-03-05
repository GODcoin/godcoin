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

struct PoBox {
    connected: Option<Recipient<msg::Connected>>,
    disconnected: Option<Recipient<msg::Disconnected>>,
    message: Recipient<msg::Message>,
}

pub struct Network {
    po_box: PoBox,
    sessions: HashMap<SessionId, SessionInfo>,
}

impl Network {
    pub fn new<T>(msg_handler: &Addr<T>) -> Self
    where
        T: Handler<msg::Message>,
        T::Context: actix::dev::ToEnvelope<T, msg::Message>,
    {
        let po_box = PoBox {
            connected: None,
            disconnected: None,
            message: msg_handler.clone().recipient(),
        };
        Network {
            po_box,
            sessions: HashMap::with_capacity(32),
        }
    }

    pub fn subscribe_connect<T: Actor>(&mut self, tx: &Addr<T>)
    where
        T: Handler<msg::Connected>,
        T::Context: actix::dev::ToEnvelope<T, msg::Connected>,
    {
        self.po_box.connected.replace(tx.clone().recipient());
    }

    pub fn subscribe_disconnect<T: Actor>(&mut self, tx: &Addr<T>)
    where
        T: Handler<msg::Disconnected>,
        T::Context: actix::dev::ToEnvelope<T, msg::Disconnected>,
    {
        self.po_box.disconnected.replace(tx.clone().recipient());
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
            NetCmd::Disconnect(id) => {
                if let Some(ses) = self.sessions.get(&id) {
                    ses.address.do_send(session::Disconnect);
                }
            }
        }
    }
}

impl Handler<SessionMsg> for Network {
    type Result = ();

    fn handle(&mut self, msg: SessionMsg, _: &mut Self::Context) {
        match msg {
            SessionMsg::Connected(ses) => {
                let id = ses.id;
                let prev = self.sessions.insert(id, ses.clone());
                assert!(prev.is_none());
                if let Some(tx) = &self.po_box.connected {
                    tx.do_send(msg::Connected(ses)).unwrap();
                }
            }
            SessionMsg::Disconnected(addr) => {
                let ses = self
                    .sessions
                    .remove(&addr)
                    .unwrap_or_else(|| panic!("Expected disconnected peer to exist: {}", addr));
                if let Some(tx) = &self.po_box.disconnected {
                    tx.do_send(msg::Disconnected(ses)).unwrap();
                }
            }
            SessionMsg::Message(ses_id, payload) => {
                // TODO: ID caching to prevent broadcast loops
                self.broadcast(&payload, ses_id);
                self.po_box
                    .message
                    .do_send(msg::Message(ses_id, payload))
                    .unwrap();
            }
        }
    }
}

pub mod msg {
    use super::*;

    #[derive(Clone, Debug)]
    pub struct Connected(pub SessionInfo);

    impl actix::Message for Connected {
        type Result = ();
    }

    #[derive(Clone, Debug)]
    pub struct Disconnected(pub SessionInfo);

    impl actix::Message for Disconnected {
        type Result = ();
    }

    #[derive(Clone, Debug)]
    pub struct Message(pub SessionId, pub Payload);

    impl actix::Message for Message {
        type Result = ();
    }
}
