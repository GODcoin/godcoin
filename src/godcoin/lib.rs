extern crate sodiumoxide;
extern crate num_bigint;
extern crate num_traits;
extern crate rocksdb;
extern crate crc32c;
extern crate bytes;
extern crate bs58;

extern crate tokio;
extern crate tokio_codec;

#[cfg(test)]
extern crate rand;

#[macro_use]
mod buf_util;

pub mod asset;
pub use self::asset::{Asset, AssetSymbol, EMPTY_GOLD, EMPTY_SILVER};

pub mod crypto;
pub use self::crypto::{KeyPair, PublicKey, PrivateKey, SigPair, Wif};

pub mod serializer;
pub use self::serializer::*;

pub mod tx;
pub use self::tx::*;

pub mod net;
pub mod blockchain;

pub fn init() -> Result<(), ()> {
    sodiumoxide::init()
}
