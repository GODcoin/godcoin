use crate::*;
use bytes::BytesMut;
use network::connect::TcpConnect;
use std::{fmt, net::SocketAddr};
use tokio::{codec::FramedRead, prelude::*};

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
    pub outbound_addr: Option<SocketAddr>,
    pub peer_addr: SocketAddr,
}

impl PeerInfo {
    #[inline]
    pub fn is_outbound(&self) -> bool {
        self.outbound_addr.is_some()
    }
}

impl fmt::Display for PeerInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{}", self.id))
    }
}

pub struct Peer<S: 'static, M: 'static + Metrics> {
    net_addr: Addr<Network<S, M>>,
    outbound_addr: Option<SocketAddr>,
    state: PeerState,
}

impl<S: 'static, M: 'static + Metrics> Peer<S, M> {
    pub fn init(conn: TcpConnect, addr: Addr<Network<S, M>>) {
        let stream = conn.0;
        let outbound_addr = conn.1;
        let (peer_addr, conn_type) = if let Some(addr) = outbound_addr {
            (addr, "outbound")
        } else {
            (stream.peer_addr().unwrap(), "inbound")
        };
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
                outbound_addr,
                state: PeerState::Disconnected(None),
            }
        });
    }

    #[inline]
    pub fn is_outbound(&self) -> bool {
        self.outbound_addr.is_some()
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
                    None => self
                        .outbound_addr
                        .map_or("Unknown".to_owned(), |v| format!("{}", v)),
                };
                warn!("[{}] Attempted to send message to disconnected peer", peer);
            }
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
            SessionMsg::Connected(ses, addr) => match &self.state {
                PeerState::Disconnected(_) => {
                    self.state = PeerState::Handshaking(ses.clone(), addr);
                    if self.is_outbound() {
                        debug!("[{}] Sending outbound handshake", addr);
                        ses.do_send(Payload {
                            id: BytesMut::from(vec![0]),
                            msg: BytesMut::new(),
                        });
                    }
                }
                _ => panic!("Peer state is invalid: {:?}", self.state),
            },
            SessionMsg::Disconnected => match &self.state {
                PeerState::Disconnected(_) => panic!("Peer state is invalid: {:?}", self.state),
                PeerState::Handshaking(_, _) => {
                    self.state = PeerState::Disconnected(None);
                }
                PeerState::Ready(_, info) => {
                    self.net_addr
                        .try_send(msg::Disconnected(info.clone()))
                        .unwrap();
                    self.state = PeerState::Disconnected(Some(info.clone()));
                }
            },
            SessionMsg::Message(msg) => match &self.state {
                PeerState::Disconnected(_) => {
                    panic!("Received message from disconnected peer: {:?}", self.state);
                }
                PeerState::Handshaking(ses, peer_addr) => {
                    let peer_addr = peer_addr.clone();
                    let info = PeerInfo {
                        id: peer_addr,
                        outbound_addr: self.outbound_addr,
                        peer_addr: peer_addr,
                    };
                    let ses = ses.clone();
                    if !self.is_outbound() {
                        self.net_addr
                        .send(msg::Handshake(info.clone(), msg))
                        .into_actor(self)
                        .map(move |res, act, ctx| {
                            match res {
                                msg::HandshakeRequest::Allow => {
                                    debug!("[{}] Sending inbound handshake", peer_addr);
                                    ses.do_send(Payload {
                                        id: BytesMut::from(vec![0]),
                                        msg: BytesMut::new(),
                                    });
                                    act.state = PeerState::Ready(ses, info.clone());
                                    act.net_addr
                                        .try_send(msg::Connected(info, ctx.address()))
                                        .unwrap();
                                }
                                msg::HandshakeRequest::Deny(reason) => {
                                    warn!("[{}] Connection rejected: {}", peer_addr, reason);
                                    ctx.address().do_send(cmd::Disconnect);
                                }
                            }
                        })
                        .map_err(move |e, _, ctx| {
                            error!(
                                "[{}] Failed to send handshake request to network: {:?}",
                                peer_addr, e
                            );
                            ctx.address().do_send(cmd::Disconnect);
                        })
                        .wait(ctx);
                    } else {
                        self.state = PeerState::Ready(ses, info.clone());
                        self.net_addr
                            .try_send(msg::Connected(info, ctx.address()))
                            .unwrap();
                    }

                }
                PeerState::Ready(_, info) => {
                    self.net_addr.try_send(msg::Message(info.id, msg)).unwrap();
                }
            },
        }
    }
}

pub mod msg {
    use super::*;

    #[derive(Message, Debug)]
    #[rtype(HandshakeRequest)]
    pub struct Handshake(pub PeerInfo, pub Payload);

    pub enum HandshakeRequest {
        Allow,
        Deny(String),
    }

    impl<A, M> actix::dev::MessageResponse<A, M> for HandshakeRequest
    where
        A: Actor,
        M: actix::Message<Result = HandshakeRequest>,
    {
        fn handle<R: actix::dev::ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
            if let Some(tx) = tx {
                tx.send(self);
            }
        }
    }

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
