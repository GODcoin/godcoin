use actix::prelude::*;
use bytes::BytesMut;
use godcoin_p2p::{peer, BasicMetrics, Network, Payload, PeerId, PeerInfo};
use log::info;
use std::{cell::RefCell, collections::HashSet, rc::Rc};

type NetAddr = Addr<Network<NetState, BasicMetrics>>;

pub struct NetState {
    net_id: usize,
    messages: HashSet<BytesMut>,
    msg_counter: Rc<RefCell<usize>>,
    msg_threshold: usize,
}

impl NetState {
    pub fn new(net_id: usize, msg_counter: Rc<RefCell<usize>>, msg_threshold: usize) -> Self {
        NetState {
            net_id,
            messages: HashSet::new(),
            msg_counter,
            msg_threshold,
        }
    }
}

pub fn connect_req(
    _: &NetAddr,
    state: &mut NetState,
    peer: PeerInfo,
    hs: Payload,
) -> peer::msg::HandshakeRequest {
    info!(
        "[net:{}] ({}) Received hs request: {:?}",
        state.net_id, peer.peer_addr, hs
    );
    peer::msg::HandshakeRequest::Allow
}

pub fn connected(_: &NetAddr, state: &mut NetState, peer: PeerInfo) {
    if peer.is_outbound() {
        info!(
            "[net:{}] Accepted connection -> {}",
            state.net_id, peer.peer_addr
        );
    } else {
        info!(
            "[net:{}] Connected to node -> {}",
            state.net_id, peer.peer_addr
        );
    }
}

pub fn disconnected(_: &NetAddr, state: &mut NetState, ses: PeerInfo) {
    info!(
        "[net:{}] Connection disconnected -> {}",
        state.net_id, ses.peer_addr
    );
}

pub fn message(_: &NetAddr, state: &mut NetState, id: PeerId, payload: &Payload) -> bool {
    info!(
        "[net:{}] Received message from {} with: {:?}",
        state.net_id, id, payload
    );
    let broadcast = state.messages.contains(&payload.id);
    *state.msg_counter.borrow_mut() += 1;
    if *state.msg_counter.borrow() == state.msg_threshold {
        info!("Threshold reached -> evicting cached messages");
        state.messages.clear();
    }
    broadcast
}
