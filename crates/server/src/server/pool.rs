use futures::channel::mpsc::Sender;
use godcoin::prelude::*;
use parking_lot::RwLock;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]
pub struct SubscriptionPool {
    clients: Arc<RwLock<HashMap<SocketAddr, Sender<Message>>>>,
}

impl SubscriptionPool {
    #[inline]
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::with_capacity(1024))),
        }
    }

    #[inline]
    pub fn insert(&self, addr: SocketAddr, tx: Sender<Message>) {
        self.clients.write().insert(addr, tx);
    }

    #[inline]
    pub fn remove(&self, addr: SocketAddr) {
        self.clients.write().remove(&addr);
    }

    pub fn broadcast(&self, msg: rpc::Response) {
        let msg = {
            let mut buf = Vec::with_capacity(65536);
            let res = Msg {
                id: u32::max_value(),
                body: Body::Response(msg),
            };
            res.serialize(&mut buf);
            Message::Binary(buf)
        };

        let clients = self.clients.read();
        for client in clients.values() {
            // Errors only occur when the other end is dropped, it is the pool managers responsibility to remove any
            // disconnected clients
            let _ = client.clone().try_send(msg.clone());
        }
    }
}

impl Default for SubscriptionPool {
    #[inline]
    fn default() -> Self {
        SubscriptionPool::new()
    }
}
