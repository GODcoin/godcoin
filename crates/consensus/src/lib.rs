mod config;
mod log;
mod net;
mod peer;

#[cfg(test)]
mod test_util;

use bytes::Bytes;
use config::Config;
use futures::{channel::mpsc, prelude::*};
use log::{Entry, Log, Storage};
use net::*;
use peer::*;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_util::codec::Framed;
use tracing::{info, info_span, trace, warn};
use tracing_futures::Instrument;

use private::Inner;

#[derive(Debug)]
pub struct Node<S: Storage> {
    inner: Arc<Mutex<Inner>>,
    log: Arc<Mutex<Log<S>>>,
    config: Config,
}

impl<S: Storage> Node<S> {
    pub fn new(config: Config, storage: S) -> Self {
        config.validate().unwrap();
        Self {
            inner: Arc::new(Mutex::new(Inner::new(config.clone()))),
            log: Arc::new(Mutex::new(Log::new(storage))),
            config,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub async fn add_peer(&self, id: u32, addr: SocketAddr) {
        let inner = &mut self.inner.lock().await;
        assert_ne!(self.config.id, id, "cannot add self as a peer");
        if inner.peers.insert(id, Peer::new(addr)).is_some() {
            panic!("Peer id {} already registered", id);
        }
    }

    pub async fn collect_peer_info(&self) -> HashMap<u32, PeerInfo> {
        let inner = self.inner.lock().await;
        let mut map = HashMap::with_capacity(inner.peers.len());
        for (id, peer) in inner.peers.iter() {
            map.insert(*id, peer.collect_info());
        }
        map
    }

    pub async fn leader(&self) -> u32 {
        self.inner.lock().await.leader()
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

    /// Returns `Some` when not a leader and entries could not be appended, otherwise returns `None`
    /// on success.
    pub async fn append_entries(&self, mut entries: Vec<Bytes>) -> Option<Vec<Bytes>> {
        let mut inner = self.inner.lock().await;
        let mut log = self.log.lock().await;
        if !inner.is_leader() {
            return Some(entries);
        }
        let index_start = log.latest_index() + 1;
        let term = inner.term();
        let entries = {
            let mut e = Vec::with_capacity(entries.len());
            for (i, data) in entries.drain(..).enumerate() {
                e.push(Entry {
                    index: index_start + i as u64,
                    term,
                    data,
                });
            }
            e
        };

        log.try_commit(entries.clone()).unwrap();
        inner.insert_outbound_entries(entries);
        None
    }

    pub async fn tick(self: &Arc<Self>) {
        let mut inner = self.inner.lock().await;
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

        let log = self.log.lock().await;
        if inner.tick_election() {
            // Election timeout has expired, start a new election
            let term = inner.term() + 1;
            let last_index = log.latest_index();
            let last_term = log.latest_term();
            inner.become_candidate(term);
            inner.broadcast_req(Request::RequestVote(RequestVoteReq {
                term,
                last_index,
                last_term,
            }));
        }

        if inner.tick_heartbeat() {
            let append_entries = AppendEntriesReq {
                term: inner.term(),
                prev_index: inner.log_last_index,
                prev_term: inner.log_last_term,
                leader_commit: log.stable_index(),
                entries: inner.take_outbound_entries(),
            };
            inner.log_last_index = log.latest_index();
            inner.log_last_term = log.latest_term();
            inner.broadcast_req(Request::AppendEntries(append_entries));
        }
    }

    pub async fn init_peer_connection(self: &Arc<Self>, stream: TcpStream) {
        let server_hs = {
            let inner = self.inner.lock().await;
            Handshake {
                peer_id: inner.config.id,
            }
        };

        let peer_addr = stream.peer_addr().unwrap();
        let mut framed = Framed::new(stream, RpcCodec::default());
        let client_hs = match Peer::perform_handshake(&mut framed, server_hs).await {
            Some(hs) => hs,
            None => return,
        };

        // This lock must never pass an await point
        let peers = &mut self.inner.lock().await.peers;
        let span = info_span!("peer", id = client_hs.peer_id, addr = ?peer_addr);
        match peers.get_mut(&client_hs.peer_id) {
            Some(peer) => {
                if peer.is_connected() {
                    // Drop the connection
                    return;
                }

                let (tx, rx) = {
                    let _entered = span.enter();
                    let (tx, rx) = {
                        let (mut tx, rx) = framed.split();
                        let (sender_tx, mut sender_rx) = mpsc::channel(32);
                        tokio::spawn(async move {
                            while let Some(msg) = sender_rx.next().await {
                                let _ = tx.send(msg).await;
                            }
                        });
                        (sender_tx, rx)
                    };

                    peer.connection_established(tx.clone());
                    info!("Connected");
                    (tx, rx)
                };

                tokio::spawn({
                    let node = Arc::clone(self);
                    let task = async move {
                        let peer_id = client_hs.peer_id;
                        node.handle_incoming_peer_msgs(peer_id, tx, rx).await;
                        info!("Disconnected");
                        // Tick the connection to signify the connection has been dropped. The
                        // result doesn't matter as we wouldn't want to perform an immediate
                        // reconnection.
                        let mut inner = node.inner.lock().await;
                        let peer = inner.peers.get_mut(&peer_id).unwrap();
                        peer.tick_connection();
                    };
                    task.instrument(span)
                });
            }
            None => {
                let _entered = span.enter();
                warn!("Connection established with an unknown id")
            }
        }
    }

    async fn handle_incoming_peer_msgs(
        self: &Arc<Self>,
        peer_id: u32,
        mut tx: ActiveConnSink,
        mut rx: ActiveConnStream,
    ) {
        while let Some(msg) = rx.next().await {
            let msg = match msg {
                Ok(msg) => msg,
                Err(e) => {
                    warn!("Error receiving message: {:?}", e);
                    break;
                }
            };
            trace!("Received msg: {:?}", msg);
            match msg.data {
                MsgKind::Handshake(_) => {
                    warn!("Unexpected handshake message");
                    break;
                }
                MsgKind::Request(req) => {
                    if let Some(res) = self.process_peer_req(peer_id, msg.id, req).await {
                        let _ = tx
                            .send(Msg {
                                id: msg.id,
                                data: MsgKind::Response(res),
                            })
                            .await;
                    }
                }
                MsgKind::Response(res) => self.process_peer_res(peer_id, res).await,
            }
        }
    }

    async fn process_peer_req(&self, peer_id: u32, msg_id: u64, req: Request) -> Option<Response> {
        let mut inner = self.inner.lock().await;
        let mut log = self.log.lock().await;
        match req {
            Request::RequestVote(req) => {
                let log_is_latest = log.is_up_to_date(req.last_index, req.last_term);
                let approved = log_is_latest && req.term >= inner.term() && inner.voted_for() == 0;
                if req.term > inner.term() || approved {
                    inner.become_follower(req.term);
                }
                if approved {
                    inner.vote(peer_id);
                }
                Some(Response::RequestVote(RequestVoteRes {
                    current_term: inner.term(),
                    approved,
                }))
            }
            Request::AppendEntries(req) => {
                if req.term < inner.term() {
                    return Some(Response::AppendEntries(AppendEntriesRes {
                        current_term: inner.term(),
                        success: false,
                        index: 0,
                    }));
                }

                inner.maybe_update_term(req.term);
                inner.assign_leader(peer_id);
                inner.received_heartbeat();
                let current_term = inner.term();

                if !inner.is_follower() {
                    inner.become_follower(current_term);
                }

                let has_entry = log.contains_entry(req.prev_term, req.prev_index);
                if has_entry {
                    log.try_commit(req.entries)
                        .expect("Failed to commit entries");
                    inner.is_syncing = false;
                } else if !inner.is_syncing {
                    let leader_id = inner.leader();
                    let peer = inner.peers.get_mut(&leader_id).unwrap();
                    if let Some(mut sender) = peer.get_sender() {
                        let id = peer.incr_next_msg_id();
                        inner.is_syncing = true;
                        let last_index = log.latest_index();
                        let last_term = log.latest_term();
                        tokio::spawn(async move {
                            let _ = sender
                                .send(Msg {
                                    id,
                                    data: MsgKind::Request(Request::LogSync(LogSyncReq {
                                        last_index,
                                        last_term,
                                    })),
                                })
                                .await;
                        });
                    }
                }

                log.stabilize_to(req.leader_commit);
                let index = log.latest_index();
                Some(Response::AppendEntries(AppendEntriesRes {
                    current_term,
                    success: has_entry,
                    index,
                }))
            }
            Request::LogSync(req) => {
                // TODO
                // (1) If peer node's index term is less than our index's term then we need to find
                // the first index of the node's index term in our log.

                let inner_locked = Arc::clone(&self.inner);
                let log_locked = Arc::clone(&self.log);
                tokio::spawn(async move {
                    let mut entries = vec![];
                    let mut last_index = req.last_index;
                    loop {
                        entries.clear();

                        // We lock inside the loop to allow other systems to continue operating
                        // during the sync process with a peer.
                        let log = log_locked.lock().await;
                        let inner = inner_locked.lock().await;

                        let latest_index = log.latest_index();
                        let stable_index = log.stable_index();
                        let complete = {
                            let mut byte_len = 0;
                            while last_index < latest_index {
                                last_index += 1;
                                let entry = log.get_entry_by_index(last_index).unwrap();
                                byte_len += entry.data.len();
                                entries.push(entry);
                                if byte_len > 1024 * 1024 * 10 {
                                    break;
                                }
                            }
                            entries.is_empty()
                        };

                        let peer = inner.peers.get(&peer_id).unwrap();
                        match peer.get_sender() {
                            Some(mut sender) => {
                                let res = Response::LogSync(LogSyncRes {
                                    leader_commit: stable_index,
                                    complete,
                                    entries: entries.clone(),
                                });
                                if sender
                                    .send(Msg {
                                        id: msg_id,
                                        data: MsgKind::Response(res),
                                    })
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            None => break,
                        }

                        if complete {
                            break;
                        }
                    }
                });

                None
            }
        }
    }

    async fn process_peer_res(&self, peer_id: u32, res: Response) {
        let mut inner = self.inner.lock().await;
        let mut log = self.log.lock().await;
        match res {
            Response::RequestVote(res) => {
                inner.maybe_update_term(res.current_term);
                if inner.is_candidate() && res.approved {
                    inner.received_vote();
                }
            }
            Response::AppendEntries(res) => {
                inner.maybe_update_term(res.current_term);
                if inner.is_leader() && res.success {
                    let peer = inner.peers.get_mut(&peer_id).unwrap();
                    peer.set_last_ack_index(res.index);

                    let next_commit = inner.next_stable_index();
                    log.stabilize_to(next_commit);
                }
            }
            Response::LogSync(res) => {
                if !inner.is_syncing {
                    warn!("Received erroneous sync request from {}", peer_id);
                    return;
                }
                if res.complete {
                    inner.is_syncing = false;
                }
                log.try_commit(res.entries).unwrap();
                log.stabilize_to(res.leader_commit);
            }
        }
    }
}

mod private {
    use super::*;

    #[derive(Debug)]
    pub struct Inner {
        pub config: Config,
        pub peers: HashMap<u32, Peer>,
        pub log_last_term: u64,
        pub log_last_index: u64,
        pub is_syncing: bool,
        outbound_entries: Vec<Entry>,
        term: u64,
        leader_id: u32,
        candidate_id: u32,
        election_delta: u32,
        election_timeout: u32,
        received_votes: u32,
        heartbeat_delta: u32,
    }

    impl Inner {
        pub fn new(config: Config) -> Self {
            let election_timeout = config.random_election_timeout();
            Self {
                config,
                peers: Default::default(),
                log_last_term: 0,
                log_last_index: 0,
                is_syncing: false,
                outbound_entries: Vec::with_capacity(32),
                term: 0,
                leader_id: 0,
                candidate_id: 0,
                election_delta: 0,
                election_timeout,
                received_votes: 0,
                heartbeat_delta: 0,
            }
        }

        pub fn broadcast_req(&mut self, req: Request) {
            for peer in self.peers.values_mut() {
                if !peer.is_connected() {
                    continue;
                }
                if let Some(mut tx) = peer.get_sender() {
                    let id = peer.incr_next_msg_id();
                    let msg = Msg {
                        id,
                        data: MsgKind::Request(req.clone()),
                    };
                    tokio::spawn(async move {
                        let _ = tx.send(msg).await;
                    });
                }
            }
        }

        pub fn insert_outbound_entries(&mut self, mut entries: Vec<Entry>) {
            self.outbound_entries.append(&mut entries);
        }

        pub fn take_outbound_entries(&mut self) -> Vec<Entry> {
            let mut new_vec = Vec::with_capacity(self.outbound_entries.capacity());
            std::mem::swap(&mut new_vec, &mut self.outbound_entries);
            new_vec
        }

        pub fn tick_election(&mut self) -> bool {
            self.election_delta += 1;
            self.election_delta > self.election_timeout
        }

        pub fn tick_heartbeat(&mut self) -> bool {
            if self.is_candidate() && self.check_quorum(self.received_votes) {
                self.become_leader();
                return true;
            } else if !self.is_leader() {
                return false;
            }
            self.heartbeat_delta += 1;
            if self.heartbeat_delta > self.config.heartbeat_timeout {
                self.heartbeat_delta = 0;
                true
            } else {
                false
            }
        }

        pub fn become_follower(&mut self, term: u64) {
            self.reset(term);
        }

        pub fn become_candidate(&mut self, term: u64) {
            self.reset(term);
            self.vote(self.config.id);
            // We always vote for ourself when becoming a candidate
            self.received_vote();
        }

        pub fn become_leader(&mut self) {
            self.reset(self.term);
            self.leader_id = self.config.id;
        }

        pub fn is_follower(&self) -> bool {
            !(self.is_candidate() || self.is_leader())
        }

        pub fn is_candidate(&self) -> bool {
            self.candidate_id == self.config.id
        }

        pub fn is_leader(&self) -> bool {
            self.leader_id == self.config.id
        }

        pub fn leader(&self) -> u32 {
            self.leader_id
        }

        pub fn term(&self) -> u64 {
            self.term
        }

        pub fn received_vote(&mut self) {
            self.received_votes += 1;
        }

        pub fn received_heartbeat(&mut self) {
            self.election_delta = 0;
        }

        pub fn vote(&mut self, candidate_id: u32) {
            assert_eq!(self.candidate_id, 0, "already voted");
            self.candidate_id = candidate_id;
        }

        pub fn voted_for(&self) -> u32 {
            self.candidate_id
        }

        pub fn assign_leader(&mut self, id: u32) {
            self.leader_id = id;
        }

        pub fn maybe_update_term(&mut self, term: u64) {
            self.term = u64::max(term, self.term);
        }

        pub fn next_stable_index(&self) -> u64 {
            assert!(
                self.is_leader(),
                "cannot retrieve next stable index without being a leader"
            );

            let quorum_cnt = self.get_quorum_count();
            let mut stable_index = 0;

            let mut indexes = Vec::with_capacity(self.peers.len());
            for peer in self.peers.values() {
                indexes.push(peer.last_ack_index());
            }
            indexes.sort_unstable();

            for last_ack in indexes.drain(..).rev() {
                // Start at 1 to include ourself, since we are the leader, we always have the
                // highest index
                let mut index_count = 1;
                for peer in self.peers.values() {
                    if peer.last_ack_index() >= last_ack {
                        index_count += 1;
                    }
                }
                if index_count > quorum_cnt {
                    stable_index = last_ack;
                    break;
                }
            }

            stable_index
        }

        #[inline]
        fn check_quorum(&self, count: u32) -> bool {
            count as usize > self.get_quorum_count()
        }

        #[inline]
        fn get_quorum_count(&self) -> usize {
            (self.peers.len() / 2) + 1
        }

        fn reset(&mut self, term: u64) {
            assert!(term >= self.term, "term cannot be smaller than ours");
            self.term = term;
            self.leader_id = 0;
            self.candidate_id = 0;
            self.election_delta = 0;
            self.election_timeout = self.config.random_election_timeout();
            self.received_votes = 0;
            self.heartbeat_delta = 0;
            self.outbound_entries.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::MemStorage;
    use std::time::Duration;
    use tokio::{stream::iter, time::delay_for};

    #[tokio::test]
    async fn peer_connected() {
        let _guard = crate::test_util::init_tracing();
        let (node_1, addr_1) = setup_node(1).await;
        let (node_2, addr_2) = setup_node(2).await;

        node_1.add_peer(node_2.config().id, addr_2).await;
        node_2.add_peer(node_1.config().id, addr_1).await;

        let conn = TcpStream::connect(addr_2).await.unwrap();
        Arc::clone(&node_1).init_peer_connection(conn).await;

        assert!(node_1.collect_peer_info().await.get(&2).unwrap().connected);
        assert!(node_2.collect_peer_info().await.get(&1).unwrap().connected);
    }

    #[tokio::test]
    async fn negotiate_a_leader() {
        let _guard = crate::test_util::init_tracing();
        let cluster = setup_cluster(3).await;

        // Limit iterations to negotiate a leader before failing
        'outer: for _ in 0..50u8 {
            for node in &cluster {
                node.tick().await;
                delay_for(Duration::from_millis(1)).await;
                let inner = node.inner.lock().await;
                if inner.is_leader() {
                    break 'outer;
                }
            }
        }

        let (leader_cnt, leader_id) = iter(&cluster)
            .fold((0u32, 0u32), |mut cnt, node| async move {
                let inner = node.inner.lock().await;
                if inner.is_leader() {
                    cnt.0 += 1;
                    cnt.1 = inner.config.id;
                }
                cnt
            })
            .await;

        let candidate_cnt = iter(&cluster)
            .fold(0u8, |mut cnt, node| async move {
                if node.inner.lock().await.is_candidate() {
                    cnt += 1;
                }
                cnt
            })
            .await;

        assert_eq!(
            candidate_cnt, 0,
            "should not have any candidates when a leader is found"
        );
        assert_eq!(leader_cnt, 1, "failed to properly negotiate a leader");

        for node in &cluster {
            assert_eq!(node.leader().await, leader_id);
        }
    }

    async fn setup_cluster(count: u32) -> Vec<Arc<Node<MemStorage>>> {
        assert!(count > 0);
        let mut nodes = Vec::with_capacity(count as usize);
        let mut addrs = Vec::with_capacity(count as usize);

        // Initialize nodes
        for id in 1..=count {
            let (node, addr) = setup_node(id).await;
            nodes.push(node);
            addrs.push(addr);
        }

        // Initialize connections
        for node in &nodes {
            let id = node.config().id;
            for (peer_node, peer_addr) in nodes.iter().zip(&addrs) {
                let peer_id = peer_node.config().id;
                if id == peer_id {
                    continue;
                }
                node.add_peer(peer_id, *peer_addr).await;
                let conn = TcpStream::connect(peer_addr).await.unwrap();
                node.init_peer_connection(conn).await;
            }
        }

        // Assert connections
        for node in &nodes {
            let peer_info = node.collect_peer_info().await;
            assert_eq!(peer_info.len(), count as usize - 1);
            for peer in peer_info.values() {
                assert!(peer.connected);
            }
        }

        nodes
    }

    async fn setup_node(id: u32) -> (Arc<Node<MemStorage>>, SocketAddr) {
        let (server, addr) = listen_random().await;

        let storage = MemStorage::default();
        let config = Config::new(id);
        let node = Arc::new(Node::new(config, storage));
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
        let listener = TcpListener::bind(addr)
            .await
            .expect("Failed to start server");
        let local_addr = listener
            .local_addr()
            .expect("Failed to get server local address");
        (listener, local_addr)
    }
}
