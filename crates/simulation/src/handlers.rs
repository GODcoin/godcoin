use actix::prelude::*;
use bytes::BytesMut;
use futures::prelude::*;
use godcoin_p2p::{cmd, peer, BasicMetrics, Network, Payload, PeerId, PeerInfo};
use log::error;
use log::info;
use std::{
    cell::RefCell,
    collections::HashSet,
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::timer::Delay;

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

pub fn connected(net: &NetAddr, state: &mut NetState, peer: PeerInfo) {
    let net_id = state.net_id;
    if !peer.is_outbound() {
        info!("[net:{}] Accepted connection -> {}", net_id, peer.peer_addr);
    } else {
        let deadline = Instant::now() + Duration::from_secs(3);
        let addr = peer.outbound_addr.unwrap();
        let net = net.clone();
        Arbiter::spawn(
            Delay::new(deadline)
                .and_then(move |_| {
                    info!("[net:{}] Disconnecting from node -> {}", net_id, addr);
                    net.do_send(cmd::Disconnect(peer.id, "timeout reached".to_owned()));
                    Ok(())
                })
                .map_err(|e| {
                    error!("Timer failed: {:?}", e);
                }),
        );
        info!("[net:{}] Connected to node -> {}", net_id, addr);
    }
}

pub fn disconnected(_: &NetAddr, state: &mut NetState, ses: PeerInfo, reason: String) {
    info!(
        "[net:{}] Connection disconnected (outbound:{}, addr: {}) -> {}",
        state.net_id,
        ses.is_outbound(),
        ses.peer_addr,
        reason,
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
