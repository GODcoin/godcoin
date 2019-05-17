#[macro_use]
pub mod util;

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
    pub use super::asset::{Asset, AssetError, AssetErrorKind};
    pub use super::blockchain::{verify, Block, Blockchain, Properties, SignedBlock};
    pub use super::crypto::{
        KeyPair, PrivateKey, PublicKey, ScriptHash, SigPair, Wif, WifError, WifErrorKind,
    };
    pub use super::net::{self, MsgRequest, MsgResponse};
    pub use super::script::{self, OpFrame, Script, ScriptEngine};
    pub use super::tx::{MintTx, OwnerTx, RewardTx, SignTx, TransferTx, Tx, TxType, TxVariant};
    pub use super::util;
}
