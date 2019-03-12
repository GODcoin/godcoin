use bytes::BytesMut;
use godcoin_p2p::{network, Payload, PeerId, PeerInfo};
use log::info;
use std::{cell::RefCell, collections::HashSet, rc::Rc};

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

pub fn connected(state: &mut NetState, ses: PeerInfo) {
    match ses.conn_type {
        network::ConnectionType::Inbound => {
            info!(
                "[net:{}] Accepted connection -> {}",
                state.net_id, ses.peer_addr
            );
        }
        network::ConnectionType::Outbound => {
            info!(
                "[net:{}] Connected to node -> {}",
                state.net_id, ses.peer_addr
            );
        }
    }
}

pub fn disconnected(state: &mut NetState, ses: PeerInfo) {
    info!(
        "[net:{}] Connection disconnected -> {}",
        state.net_id, ses.peer_addr
    );
}

pub fn message(state: &mut NetState, id: PeerId, payload: &Payload) -> bool {
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