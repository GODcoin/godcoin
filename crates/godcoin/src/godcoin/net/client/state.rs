use std::sync::{Arc, atomic::AtomicBool};
use std::net::SocketAddr;
use parking_lot::Mutex;
use std::cell::RefCell;

use super::{Sender, PeerType};
use fut_util::channel;

#[derive(Clone)]
pub struct ConnectState {
    pub addr: SocketAddr,
    pub peer_type: PeerType,
    pub stay_connected: Arc<AtomicBool>,
    pub inner: Arc<Mutex<RefCell<Option<InternalState>>>>
}

#[derive(Clone)]
pub struct InternalState {
    pub sender: Sender,
    pub notifier: channel::ChannelSender<()>
}

impl ConnectState {
    pub fn new(addr: SocketAddr, peer_type: PeerType) -> ConnectState {
        ConnectState {
            addr,
            peer_type,
            stay_connected: Arc::new(AtomicBool::new(true)),
            inner: Arc::new(Mutex::new(RefCell::new(None)))
        }
    }
}
