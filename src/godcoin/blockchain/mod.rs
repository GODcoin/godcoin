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
    indexer: Arc<Indexer>,
    store: Mutex<RefCell<BlockStore>>
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

    #[inline(always)]
    pub fn get_bond(&self, minter: &PublicKey) -> Option<BondTx> {
        self.indexer.get_bond(minter)
    }

    #[inline(always)]
    pub fn get_chain_height(&self) -> u64 {
        self.indexer.get_chain_height()
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

    pub fn insert_block(&self, block: SignedBlock) -> Result<(), &'static str> {
        self.verify_block(&block, &self.get_chain_head())?;

        let lock = self.store.lock().unwrap();
        let store = &mut lock.borrow_mut();
        store.insert(block);

        Ok(())
    }

    ///
    /// Create and store a generated genesis block. By default the provided
    /// `minter_key` will be the genesis minter and will stake with a small
    /// amount of GOLD tokens.
    ///
    pub fn create_genesis_block(&mut self, minter_key: &KeyPair) {
        use sodiumoxide::crypto::hash::sha256::Digest;

        info!("=> Generating new block chain");
        let staker_key = KeyPair::gen_keypair();
        info!("=> Staker private key: {}", staker_key.1.to_wif());
        info!("=> Staker public key: {}", staker_key.0.to_wif());

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

    fn verify_block(&self, block: &SignedBlock, prev_block: &SignedBlock) -> Result<(), &'static str> {
        if prev_block.height + 1 != block.height {
            return Err("invalid block height")
        } else if !block.verify_tx_merkle_root() {
            return Err("invalid merkle root")
        } else if !block.verify_previous_hash(prev_block) {
            return Err("invalid previous hash")
        }

        if self.indexer.get_bond(&block.sig_pair.pub_key).is_none() {
            return Err("bond not found")
        } if !block.sig_pair.verify(block.calc_hash().as_ref()) {
            return Err("invalid bond signature")
        }

        for tx in &block.transactions {
            self.verify_tx(tx)?;
        }

        Ok(())
    }

    fn verify_tx(&self, tx: &TxVariant) -> Result<(), &'static str> {
        // TODO
        Ok(())
    }
}
