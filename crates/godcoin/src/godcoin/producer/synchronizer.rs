use tokio::prelude::*;
use net::peer_pool::*;
use std::sync::Arc;
use blockchain::*;

pub struct Synchronizer {
    chain: Arc<Blockchain>,
    pool: Arc<PeerPool>
}

impl Synchronizer {
    pub fn new(chain: Arc<Blockchain>, pool: Arc<PeerPool>) -> Synchronizer {
        Synchronizer {
            chain,
            pool
        }
    }

    pub fn start(&self) {
        // TODO
    }
}
