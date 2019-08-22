use std::time::{SystemTime, UNIX_EPOCH};

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

pub fn get_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

pub mod prelude {
    pub use super::asset::{self, Asset, AssetError, AssetErrorKind};
    pub use super::blockchain::{
        index::IndexStatus, verify, AddressInfo, Block, BlockFilter, BlockHeader, BlockHeaderV0,
        BlockV0, Blockchain, FilteredBlock, Properties,
    };
    pub use super::crypto::{
        KeyPair, PrivateKey, PublicKey, ScriptHash, SigPair, Wif, WifError, WifErrorKind,
    };
    pub use super::net::{self, RequestBody, ResponseBody};
    pub use super::script::{self, OpFrame, Script, ScriptEngine};
    pub use super::tx::{
        MintTx, OwnerTx, RewardTx, TransferTx, Tx, TxId, TxPool, TxPrecompData, TxVariant,
        TxVariantV0,
    };
}
