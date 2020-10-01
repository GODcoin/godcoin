use super::{net::*, Handshake, Msg, RpcCodec};
use futures::{channel::mpsc, prelude::*, stream::SplitStream};
use rand::Rng;
use std::{net::SocketAddr, time::Duration};
use tokio::{net::TcpStream, time::timeout};
use tokio_util::codec::Framed;

pub type ActiveConnSink = mpsc::Sender<Msg>;
pub type ActiveConnStream = SplitStream<Framed<TcpStream, RpcCodec>>;
pub type ConnFrame = Framed<TcpStream, RpcCodec>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerInfo {
    pub address: SocketAddr,
    pub connected: bool,
}

#[derive(Debug)]
pub struct Peer {
    /// The outbound address for establishing connections.
    address: SocketAddr,
    /// Whether the peer has an established connection, this includes after the handshake is
    /// performed.
    connection_sink: Option<ActiveConnSink>,
    /// The required ticks before another connection can be attempted.
    attempt_timeout: u64,
    /// Ticks elapsed without a successful connection.
    attempt_delta: u64,
    /// Attempted tries to establish a connection.
    tries: u32,
    /// The next trackable message ID that can be sent over the network.
    next_msg_id: u64,
    /// Last known index this peer has acknowledged.
    last_ack_index: u64,
}

impl Peer {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            connection_sink: None,
            attempt_timeout: next_connect_time(0),
            attempt_delta: 0,
            tries: 0,
            next_msg_id: 0,
            last_ack_index: 0,
        }
    }

    pub fn collect_info(&self) -> PeerInfo {
        PeerInfo {
            address: self.address,
            connected: self.is_connected(),
        }
    }

    #[inline]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    #[inline]
    pub fn is_connected(&self) -> bool {
        self.connection_sink.is_some()
    }

    pub fn last_ack_index(&self) -> u64 {
        self.last_ack_index
    }

    pub fn set_last_ack_index(&mut self, index: u64) {
        self.last_ack_index = u64::max(self.last_ack_index, index);
    }

    pub fn incr_next_msg_id(&mut self) -> u64 {
        let id = self.next_msg_id;
        self.next_msg_id += 1;
        id
    }

    pub fn get_sender(&self) -> Option<ActiveConnSink> {
        self.connection_sink.clone()
    }

    /// Returns whether a connection should be established or not. This should only be called when
    /// a connection needs to be established.
    pub fn tick_connection(&mut self) -> bool {
        self.connection_sink = None;
        self.attempt_delta += 1;
        if self.attempt_delta > self.attempt_timeout {
            self.attempt_delta = 0;
            self.attempt_timeout = next_connect_time(self.tries);
            self.tries += 1;
            return true;
        }

        false
    }

    pub fn connection_established(&mut self, sink: ActiveConnSink) {
        self.connection_sink = Some(sink);
        self.attempt_timeout = next_connect_time(0);
        self.attempt_delta = 0;
        self.tries = 0;
    }

    pub async fn perform_handshake(framed: &mut ConnFrame, hs: Handshake) -> Option<Handshake> {
        if framed
            .send(Msg {
                id: 0,
                data: MsgKind::Handshake(hs),
            })
            .await
            .is_err()
        {
            return None;
        }

        let msg = timeout(Duration::from_secs(1), framed.next()).await;
        if let Ok(Some(Ok(Msg {
            data: MsgKind::Handshake(hs),
            ..
        }))) = msg
        {
            Some(hs)
        } else {
            None
        }
    }
}

fn next_connect_time(tries: u32) -> u64 {
    let mut time = 2u64.pow(u32::min(tries, 7));
    time += rand::thread_rng().gen_range(0, 30);
    time
}
