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

pub fn get_epoch_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub mod prelude {
    pub use super::asset::{self, Asset, AssetError, AssetErrorKind};
    pub use super::blockchain::{
        self, index::IndexStatus, AddressInfo, Block, BlockFilter, BlockHeader, BlockHeaderV0,
        BlockV0, Blockchain, FilteredBlock, LogEntry, Properties, Receipt,
    };
    pub use super::crypto::{
        DoubleSha256, KeyPair, PrivateKey, PublicKey, ScriptHash, SigPair, Wif, WifError,
        WifErrorKind,
    };
    pub use super::net::{self, rpc, Body, Msg};
    pub use super::script::{self, OpFrame, Script, ScriptEngine};
    pub use super::tx::{
        MintTx, OwnerTx, RewardTx, TransferTx, Tx, TxId, TxPool, TxPrecompData, TxVariant,
        TxVariantV0,
    };
}
