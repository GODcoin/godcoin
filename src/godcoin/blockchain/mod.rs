use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::{Mutex, Arc};
use std::cell::RefCell;
use std::str::FromStr;
use std::path::*;

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
    pub indexer: Arc<Indexer>,
    pub store: Mutex<RefCell<BlockStore>>
}

impl Blockchain {

    ///
    /// Creates a new `Blockchain` with an associated indexer and backing
    /// storage is automatically created based on the given `path`.
    ///
    pub fn new(path: &Path) -> Blockchain {
        let indexer = Arc::new(Indexer::new(&Path::join(path, "index")));
        let store = BlockStore::new(&Path::join(path, "blklog"), Arc::clone(&indexer));
        Blockchain {
            indexer,
            store: Mutex::new(RefCell::new(store))
        }
    }

    pub fn get_chain_head(&self) -> Arc<SignedBlock> {
        let lock = self.store.lock().unwrap();
        let store = lock.borrow();
        let height = store.get_chain_height();
        store.get(height).expect("Failed to get blockchain head")
    }

    pub fn get_block(&self, height: u64) -> Option<Arc<SignedBlock>> {
        let lock = self.store.lock().unwrap();
        let store = lock.borrow();
        store.get(height)
    }

    pub fn insert_block(&self, block: SignedBlock) {
        let lock = self.store.lock().unwrap();
        let store = &mut lock.borrow_mut();
        store.insert(block);
    }

    ///
    /// Create and store a generated genesis block. By default the provided
    /// `minter_key` will be the genesis minter and will stake with a small
    /// amount of GOLD tokens.
    ///
    pub fn create_genesis_block(&mut self, minter_key: &KeyPair) {
        use sodiumoxide::crypto::hash::sha256::Digest;

        println!("=> Generating new block chain");
        let staker_key = KeyPair::gen_keypair();
        println!("=> Staker private key: {}", staker_key.1.to_wif());
        println!("=> Staker public key: {}", staker_key.0.to_wif());

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;

        let bond_tx = BondTx {
            base: Tx {
                tx_type: TxType::BOND,
                fee: Asset::from_str("0 GOLD").unwrap(),
                timestamp,
                signature_pairs: Vec::new()
            },
            minter: minter_key.0.clone(),
            staker: staker_key.0.clone(),
            bond_fee: EMPTY_GOLD,
            stake_amt: Asset::from_str("1 GOLD").unwrap()
        };

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
            vec.push(TxVariant::BondTx(bond_tx.clone()));
            vec
        };

        let block = (Block {
            height: 0,
            previous_hash: Digest::from_slice(&[0u8; 32]).unwrap(),
            tx_merkle_root: Digest::from_slice(&[0u8; 32]).unwrap(),
            timestamp: timestamp as u32,
            transactions
        }).sign(&minter_key);

        let lock = self.store.lock().unwrap();
        let store = &mut lock.borrow_mut();
        store.insert_genesis(block);
        self.indexer.set_bond(&bond_tx);
    }
}
