#[macro_use]
mod buf_util;

pub mod asset;
pub mod crypto;
pub mod tx;

pub mod blockchain;
pub mod constants;
pub mod script;
pub mod serializer;

pub fn init() -> Result<(), ()> {
    sodiumoxide::init()
}

pub mod prelude {
    pub use super::blockchain::{Block, Blockchain, SignedBlock};
    pub use super::crypto::{KeyPair, PrivateKey, PublicKey, SigPair, Wif};
}
