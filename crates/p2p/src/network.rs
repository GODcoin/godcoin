use tokio::{
    net::{TcpListener, TcpStream},
    prelude::*
};
use std::collections::HashMap;
use super::server::Server;
use std::net::SocketAddr;
use crate::*;

pub enum NetCmd {
    Listen(SocketAddr),
    Connect(SocketAddr)
}

impl Message for NetCmd {
    type Result = ();
}

pub enum NetMsg {
    Connected,
    Disconnected
}

impl Message for NetMsg {
    type Result = ();
}

pub struct Network {
    sessions: HashMap<SocketAddr, SessionInfo>
}

impl Network {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Network {
    fn default() -> Self {
        Network {
            sessions: HashMap::with_capacity(32)
        }
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
                    info!("Accepting inbound connections on {}", bind_addr);
                    ctx.add_stream(s.incoming());
                    Server {
                        recipient
                    }
                });
            },
            NetCmd::Connect(addr) => {
                let rx = ctx.address().recipient();
                Arbiter::spawn(TcpStream::connect(&addr).and_then(|s| {
                    Session::init(rx, ConnectionType::Outbound, s);
                    Ok(())
                }).map_err(move |e| {
                    warn!("[{}] Failed to connect to peer: {:?}", addr, e);
                }));
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
                let prev = self.sessions.insert(addr, ses);
                assert!(prev.is_none());
                info!("[{}] Connected!", addr);
            },
            SessionMsg::Disconnected(addr) => {
                self.sessions
                    .remove(&addr)
                    .unwrap_or_else(|| {
                        panic!("Expected disconnected peer to exist: {}", addr)
                    });
                info!("[{}] Disconnected!", addr);
            }
        }
    }
}
