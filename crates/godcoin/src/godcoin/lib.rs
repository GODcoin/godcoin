#[macro_use]
mod buf_util;

pub mod asset;
pub use self::asset::{Asset, AssetSymbol, Balance, EMPTY_GOLD, EMPTY_SILVER};

pub mod crypto;
pub use self::crypto::{KeyPair, PrivateKey, PublicKey, SigPair, Wif};

pub mod serializer;
pub use self::serializer::*;

pub mod tx;
pub use self::tx::*;

pub mod blockchain;
pub mod constants;
pub mod producer;
pub mod script;

pub fn init() -> Result<(), ()> {
    sodiumoxide::init()
}
