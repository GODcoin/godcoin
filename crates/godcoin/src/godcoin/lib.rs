#[macro_use]
mod buf_util;

pub mod asset;
pub mod crypto;
pub mod tx;

pub mod blockchain;
pub mod constants;
pub mod net;
pub mod script;
pub mod serializer;

pub fn init() -> Result<(), ()> {
    sodiumoxide::init()
}

pub mod prelude {
    pub use super::asset::{Asset, AssetError, AssetErrorKind, AssetSymbol};
    pub use super::blockchain::{Block, Blockchain, Properties, SignedBlock};
    pub use super::crypto::{KeyPair, PrivateKey, PublicKey, ScriptHash, SigPair, Wif};
    pub use super::tx::{OwnerTx, RewardTx, TransferTx, Tx, TxType, TxVariant};
}
