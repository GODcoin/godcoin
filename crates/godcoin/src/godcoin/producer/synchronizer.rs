use std::sync::Arc;

use crate::net::peer_pool::*;
use crate::blockchain::*;

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
