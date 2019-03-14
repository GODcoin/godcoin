pub mod metrics;
pub mod network;
pub mod peer;

mod protocol;
mod server;

pub use protocol::Payload;
pub use metrics::*;
pub use network::{cmd, Network};
pub use peer::{Peer, PeerId, PeerInfo};

use actix::prelude::*;
use protocol::Codec;
use log::{debug, error, warn};

pub fn init() {
    sodiumoxide::init().unwrap();
}
