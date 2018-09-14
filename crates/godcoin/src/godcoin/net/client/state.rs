use std::sync::{Arc, atomic::AtomicBool};
use std::net::SocketAddr;
use parking_lot::Mutex;
use std::cell::RefCell;

use super::{Sender, ClientType};
use fut_util::channel;

#[derive(Clone)]
pub struct ConnectState {
    pub addr: SocketAddr,
    pub client_type: ClientType,
    pub stay_connected: Arc<AtomicBool>,
    pub inner: Arc<Mutex<RefCell<Option<InternalState>>>>
}

#[derive(Clone)]
pub struct InternalState {
    pub sender: Sender,
    pub notifier: channel::ChannelSender<()>
}

impl ConnectState {
    pub fn new(addr: SocketAddr, client_type: ClientType) -> ConnectState {
        ConnectState {
            addr,
            client_type,
            stay_connected: Arc::new(AtomicBool::new(true)),
            inner: Arc::new(Mutex::new(RefCell::new(None)))
        }
    }
}
