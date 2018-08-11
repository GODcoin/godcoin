use std::time::{SystemTime, UNIX_EPOCH};
use std::str::FromStr;
use std::cell::RefCell;
use std::path::*;
use std::rc::Rc;

pub mod block;
pub mod index;
pub mod store;

pub use self::store::BlockStore;
pub use self::index::Indexer;
pub use self::block::*;

use crypto::*;
use asset::*;
use tx::*;

pub struct Blockchain {
    pub indexer: Rc<Indexer>,
    pub store: RefCell<BlockStore>
}

impl Blockchain {

    ///
    /// Creates a new `Blockchain` with an associated indexer and backing
    /// storage is automatically created based on the given `path`.
    ///
    pub fn new(path: &Path) -> Blockchain {
        let indexer = Rc::new(Indexer::new(&Path::join(path, "index")));
        let store = BlockStore::new(&Path::join(path, "blklog"), Rc::clone(&indexer));
        Blockchain {
            indexer,
            store: RefCell::new(store)
        }
    }

    ///
    /// Create and store a generated genesis block. By default the provided
    /// `minter_key` will be the genesis minter and will stake with a small
    /// amount of GOLD tokens.
    ///
    pub fn create_genesis_block(&mut self, minter_key: &KeyPair) {
        use sodiumoxide::crypto::hash::sha256::Digest;

        let store = &mut *self.store.borrow_mut();
        println!("=> Generating new block chain");
        let staker_key = KeyPair::gen_keypair();
        println!("=> Staker private key: {}", staker_key.1.to_wif());
        println!("=> Staker public key: {}", staker_key.0.to_wif());

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
        let transactions = {
            let mut vec = Vec::new();
            vec.push(TxVariant::RewardTx(RewardTx {
                base: Tx {
                    tx_type: TxType::REWARD,
                    fee: Asset::from_str("0 GOLD").unwrap(),
                    timestamp,
                    signature_pairs: Vec::new()
                },
                to: staker_key.0.clone(),
                rewards: vec![Asset::from_str("1 GOLD").unwrap()]
            }));
            vec.push(TxVariant::BondTx(BondTx {
                base: Tx {
                    tx_type: TxType::BOND,
                    fee: Asset::from_str("0 GOLD").unwrap(),
                    timestamp,
                    signature_pairs: Vec::new()
                },
                minter: staker_key.0.clone(),
                staker: staker_key.0.clone(),
                bond_fee: EMPTY_GOLD,
                stake_amt: Asset::from_str("1 GOLD").unwrap()
            }));
            vec
        };

        let block = (Block {
            height: 0,
            previous_hash: Digest::from_slice(&[0u8; 32]).unwrap(),
            tx_merkle_root: Digest::from_slice(&[0u8; 32]).unwrap(),
            timestamp: timestamp as u32,
            transactions
        }).sign(&minter_key);
        store.insert_genesis(block);
    }
}
