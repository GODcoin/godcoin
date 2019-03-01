// TODO remove dead_code allowance
#![allow(dead_code)]

use std::sync::Arc;

use crate::blockchain::*;
use crate::net_v1::peer_pool::*;

pub struct Synchronizer {
    chain: Arc<Blockchain>,
    pool: Arc<PeerPool>,
}

impl Synchronizer {
    pub fn new(chain: Arc<Blockchain>, pool: Arc<PeerPool>) -> Synchronizer {
        Synchronizer { chain, pool }
    }

    pub fn start(&self) {
        // TODO
    }
}
