mod config;
mod net;
mod peer;

use config::Config;
use futures::prelude::*;
use net::*;
use parking_lot::Mutex;
use peer::*;
use std::{collections::HashMap, sync::Arc};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::Framed;
use tracing::{info, warn};
use std::net::SocketAddr;

use private::Inner;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Leader,
    Candidate,
    Follower,
}

#[derive(Debug)]
pub struct Node {
    inner: Mutex<Inner>,
}

impl Node {
    pub fn new(config: Config) -> Self {
        config.validate().unwrap();
        Self {
            inner: Mutex::new(Inner::new(config)),
        }
    }

    pub fn config(&self) -> Config {
        self.inner.lock().config.clone()
    }

    pub fn add_peer(&self, id: u32, addr: SocketAddr) {
        let peers = &mut self.inner.lock().peers;
        if peers.insert(id, Peer::new(addr)).is_some() {
            panic!("Peer id {} already registered", id);
        }
    }

    pub fn collect_peer_info(&self) -> HashMap<u32, PeerInfo> {
        let inner = self.inner.lock();
        let mut map = HashMap::with_capacity(inner.peers.len());
        for (id, peer) in inner.peers.iter() {
            map.insert(*id, peer.collect_info());
        }
        map
    }

    pub async fn listen_forever(self: Arc<Self>, mut listener: TcpListener) {
        loop {
            let stream = listener.accept().await;
            if let Ok((stream, _)) = stream {
                let node = Arc::clone(&self);
                tokio::spawn(async move {
                    node.init_peer_connection(stream).await;
                });
            }
        }
    }

    pub fn tick(self: &Arc<Self>) {
        let mut inner = self.inner.lock();
        for peer in inner.peers.values_mut() {
            if !peer.is_connected() && peer.tick_connection() {
                let peer_addr = peer.address();
                let node = Arc::clone(&self);
                tokio::spawn(async move {
                    let stream = TcpStream::connect(peer_addr).await;
                    if let Ok(stream) = stream {
                        node.init_peer_connection(stream).await;
                    }
                });
            }
        }
    }

    pub async fn init_peer_connection(self: &Arc<Self>, stream: TcpStream) {
        let server_hs = {
            let inner = self.inner.lock();
            Handshake { peer_id: inner.config.id }
        };

        let peer_addr = stream.peer_addr().unwrap();
        let mut framed = Framed::new(stream, RpcCodec::default());
        let client_hs = match Peer::perform_handshake(&mut framed, server_hs).await {
            Some(hs) => hs,
            None => return,
        };

        // This lock must never pass an await point
        let peers = &mut self.inner.lock().peers;
        match peers.get_mut(&client_hs.peer_id) {
            Some(peer) => {
                if peer.is_connected() {
                    // Drop the connection
                    return;
                }
                let peer_id = client_hs.peer_id;
                info!(peer_id, ?peer_addr, "Peer connected");
                let (tx, mut rx) = framed.split();
                peer.connection_established(tx);
                tokio::spawn(async move {
                    while let Some(msg) = rx.next().await {
                        // TODO setup peer msg handling
                        info!(peer_id, "Received msg: {:?}", msg);
                    }
                });
            }
            None => warn!(
                peer_id = client_hs.peer_id,
                ?peer_addr,
                "Peer connected with unknown id"
            ),
        }
    }
}

mod private {
    use super::*;

    #[derive(Debug)]
    pub struct Inner {
        pub config: Config,
        pub peers: HashMap<u32, Peer>,
    }

    impl Inner {
        pub fn new(config: Config) -> Self {
            Self {
                config,
                peers: Default::default(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn peer_connected() {
        let (node_1, addr_1) = setup_node(1).await;
        let (node_2, addr_2) = setup_node(2).await;

        node_1.add_peer(node_2.config().id, addr_2);
        node_2.add_peer(node_1.config().id, addr_1);

        let conn = TcpStream::connect(addr_2).await.unwrap();
        Arc::clone(&node_1).init_peer_connection(conn).await;

        assert!(node_1.collect_peer_info().get(&2).unwrap().connected);
        assert!(node_2.collect_peer_info().get(&1).unwrap().connected);
    }

    async fn setup_node(id: u32) -> (Arc<Node>, SocketAddr) {
        let (server, addr) = listen_random().await;

        let config = Config::new(id);
        let node = Arc::new(Node::new(config));
        tokio::spawn({
            let node = Arc::clone(&node);
            async move {
                node.listen_forever(server).await;
            }
        });

        (node, addr)
    }

    async fn listen_random() -> (TcpListener, SocketAddr) {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TcpListener::bind(addr).await.expect("Failed to start server");
        let local_addr = listener.local_addr().expect("Failed to get server local address");
        (listener, local_addr)
    }
}
