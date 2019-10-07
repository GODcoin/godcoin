use futures::sync::mpsc::UnboundedSender;
use godcoin::{net::Response, prelude::*};
use parking_lot::RwLock;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]
pub struct SubscriptionPool {
    clients: Arc<RwLock<HashMap<SocketAddr, UnboundedSender<Message>>>>,
}

impl SubscriptionPool {
    #[inline]
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::with_capacity(1024))),
        }
    }

    #[inline]
    pub fn insert(&self, addr: SocketAddr, tx: UnboundedSender<Message>) {
        self.clients.write().insert(addr, tx);
    }

    #[inline]
    pub fn remove(&self, addr: SocketAddr) {
        self.clients.write().remove(&addr);
    }

    pub fn broadcast(&self, msg: ResponseBody) {
        let msg = {
            let mut buf = Vec::with_capacity(65536);
            let res = Response {
                id: u32::max_value(),
                body: msg.clone(),
            };
            res.serialize(&mut buf);
            Message::Binary(buf)
        };

        let clients = self.clients.read();
        for client in clients.values() {
            // Errors only occur when the other end is dropped, it is the pool managers responsibility to remove any
            // disconnected clients
            let _ = client.unbounded_send(msg.clone());
        }
    }
}

impl Default for SubscriptionPool {
    #[inline]
    fn default() -> Self {
        SubscriptionPool::new()
    }
}
