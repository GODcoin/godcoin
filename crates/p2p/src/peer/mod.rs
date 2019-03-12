use crate::{*, network::connect::ConnectionType};
use network::connect::TcpConnect;
use tokio::{codec::FramedRead, prelude::*};
use std::{
    net::SocketAddr,
    fmt,
};
use bytes::BytesMut;

pub mod session;
use session::*;

pub type PeerId = SocketAddr;

#[derive(Clone, PartialEq)]
pub enum PeerState {
    Disconnected(Option<PeerInfo>), // Potentially last known PeerInfo
    Handshaking(Addr<Session>, SocketAddr),
    Ready(Addr<Session>, PeerInfo),
}

impl fmt::Debug for PeerState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PeerState::Disconnected(info) => f.debug_tuple("Disconnected").field(&info).finish(),
            PeerState::Handshaking(_, addr) => f.debug_tuple("Handshaking").field(&addr).finish(),
            PeerState::Ready(_, info) => f.debug_tuple("Ready").field(&info).finish(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PeerInfo {
    pub id: PeerId,
    pub conn_type: ConnectionType,
    pub peer_addr: SocketAddr,
}

impl fmt::Display for PeerInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{}", self.id))
    }
}

pub struct Peer<S: 'static, M: 'static + Metrics> {
    net_addr: Addr<Network<S, M>>,
    conn_type: ConnectionType,
    outbound_addr: Option<SocketAddr>,
    state: PeerState,
}

impl<S: 'static, M: 'static + Metrics> Peer<S, M> {
    pub fn init(conn: TcpConnect, addr: Addr<Network<S, M>>) {
        let stream = conn.0;
        let conn_type = conn.1;
        let peer_addr = stream.peer_addr().unwrap();
        debug!("[{}] Accepted {} socket connection", peer_addr, conn_type);
        Peer::create(move |ctx| {
            let peer_tx = ctx.address().recipient();
            Session::create(move |ctx| {
                let (r, w) = stream.split();
                ctx.add_stream(FramedRead::new(r, Codec::new()));
                Session {
                    recipient: peer_tx,
                    writer: actix::io::FramedWrite::new(w, Codec::new(), ctx),
                    peer_addr,
                }
            });

            Peer {
                net_addr: addr,
                conn_type,
                outbound_addr: None,
                state: PeerState::Disconnected(None),
            }
        });
    }
}

impl<S: 'static, M: 'static + Metrics> Actor for Peer<S, M> {
    type Context = Context<Self>;
}

impl<S: 'static, M: 'static + Metrics> Handler<Payload> for Peer<S, M> {
    type Result = ();

    fn handle(&mut self, msg: Payload, _: &mut Self::Context) {
        match &self.state {
            PeerState::Disconnected(info) => {
                let peer = match info {
                    Some(info) => format!("{}", info),
                    None => self.outbound_addr.map_or("Unknown".to_owned(), |v| format!("{}", v))
                };
                warn!("[{}] Attempted to send message to disconnected peer", peer);
            },
            PeerState::Handshaking(addr, _) | PeerState::Ready(addr, _) => {
                addr.do_send(msg);
            }
        }
    }
}

impl<S: 'static, M: 'static + Metrics> Handler<cmd::Disconnect> for Peer<S, M> {
    type Result = ();

    fn handle(&mut self, _: cmd::Disconnect, ctx: &mut Self::Context) {
        match &self.state {
            PeerState::Disconnected(_) => (),
            PeerState::Handshaking(addr, _) | PeerState::Ready(addr, _) => {
               addr.do_send(session::cmd::Disconnect);
            }
        }
        ctx.stop();
    }
}

impl<S: 'static, M: 'static + Metrics> Handler<SessionMsg> for Peer<S, M> {
    type Result = ();

    fn handle(&mut self, msg: SessionMsg, ctx: &mut Self::Context) {
        match msg {
            SessionMsg::Connected(ses, addr) => {
                match &self.state {
                    PeerState::Disconnected(_) => {
                        self.state = PeerState::Handshaking(ses.clone(), addr);
                        if self.conn_type == ConnectionType::Outbound {
                            debug!("[{}] Sending outbound handshake", addr);
                            ses.do_send(Payload {
                                id: BytesMut::from(vec![0]),
                                msg: BytesMut::new()
                            });
                        }
                    }
                    _ => panic!("Peer state is invalid: {:?}", self.state)
                }
            },
            SessionMsg::Disconnected => {
                match &self.state {
                    PeerState::Disconnected(_) => panic!("Peer state is invalid: {:?}", self.state),
                    PeerState::Handshaking(_, _) => {
                        self.state = PeerState::Disconnected(None);
                    },
                    PeerState::Ready(_, info) => {
                        self.net_addr.try_send(msg::Disconnected(info.clone())).unwrap();
                        self.state = PeerState::Disconnected(Some(info.clone()));
                    }
                }
            },
            SessionMsg::Message(msg) => {
                match &self.state {
                    PeerState::Disconnected(_) => {
                        panic!("Received message from disconnected peer: {:?}", self.state);
                    },
                    PeerState::Ready(_, info) => {
                        self.net_addr.try_send(msg::Message(info.id, msg)).unwrap();
                    },
                    PeerState::Handshaking(ses, peer_addr) => {
                        if self.conn_type == ConnectionType::Inbound {
                            debug!("[{}] Sending inbound handshake", peer_addr);
                            ses.do_send(Payload {
                                id: BytesMut::from(vec![0]),
                                msg: BytesMut::new()
                            });
                        }
                        let info = PeerInfo {
                            id: *peer_addr,
                            conn_type: self.conn_type,
                            peer_addr: *peer_addr,
                        };
                        self.state = PeerState::Ready(ses.clone(), info.clone());
                        self.net_addr.try_send(msg::Connected(info, ctx.address())).unwrap();
                    }
                }
            }
        }
    }
}

pub mod msg {
    use super::*;

    #[derive(Message, Debug)]
    pub struct Handshake(pub SocketAddr, pub Payload);

    #[derive(Message)]
    pub struct Connected<S: 'static, M: 'static + Metrics>(pub PeerInfo, pub Addr<Peer<S, M>>);

    #[derive(Message)]
    pub struct Disconnected(pub PeerInfo);

    #[derive(Message)]
    pub struct Message(pub PeerId, pub Payload);
}

pub mod cmd {
    use super::*;

    #[derive(Message)]
    pub struct Disconnect;
}