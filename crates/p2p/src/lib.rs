pub mod metrics;
pub mod network;
pub mod peer;

mod protocol;
mod server;

pub use metrics::*;
pub use network::{cmd, Network};
pub use peer::{Peer, PeerId, PeerInfo};
pub use protocol::Payload;

use actix::prelude::*;
use log::{debug, error, warn};
use protocol::{Codec, ProtocolMsg};

pub fn init() {
    sodiumoxide::init().unwrap();
}
