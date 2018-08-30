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

use asset::{self, Asset, Balance, EMPTY_GOLD};
use crypto::*;
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

    pub fn get_total_fee(&self, addr: &PublicKey) -> Option<Balance> {
        let net_fee = self.get_network_fee()?;
        let addr_fee = self.get_address_fee(addr)?;
        Some(Balance {
            gold: net_fee.gold.add(&addr_fee.gold)?,
            silver: net_fee.silver.add(&addr_fee.silver)?
        })
    }

    pub fn get_address_fee(&self, addr: &PublicKey) -> Option<Balance> {
        use constants::*;
        let mut delta = 0;
        let mut tx_count = 1;

        let head = self.get_chain_height();
        for i in (0..head + 1).rev() {
            delta += 1;
            let block = self.get_block(i).unwrap();
            for tx in &block.transactions {
                let has_match = match tx {
                    TxVariant::RewardTx(_) => { false },
                    TxVariant::BondTx(tx) => { &tx.staker == addr },
                    TxVariant::TransferTx(tx) => { &tx.from == addr }
                };
                if has_match { tx_count += 1; }
            }
            if delta == FEE_RESET_WINDOW { break; }
        }

        let prec = asset::MAX_PRECISION;
        let gold = GOLD_FEE_MIN.mul(&GOLD_FEE_MULT.pow(tx_count as u16, prec)?, prec)?;
        let silver = SILVER_FEE_MIN.mul(&SILVER_FEE_MULT.pow(tx_count as u16, prec)?, prec)?;
        Some(Balance { gold, silver })
    }

    pub fn get_network_fee(&self) -> Option<Balance> {
        // The network fee adjusts every 5 blocks so that users have a bigger time
        // frame to confirm the fee they want to spend without suddenly changing.
        use constants::*;
        let max_height = self.get_chain_height();
        let max_height = max_height - (max_height % 5);
        let min_height = if max_height > NETWORK_FEE_AVG_WINDOW {
            max_height - NETWORK_FEE_AVG_WINDOW
        } else {
            0
        };

        let mut tx_count: u64 = 1;
        for i in min_height..max_height + 1 {
            tx_count += self.get_block(i).unwrap().transactions.len() as u64;
        }
        if tx_count > u64::from(u16::max_value()) { return None }

        let prec = asset::MAX_PRECISION;
        let gold = GOLD_FEE_MIN.mul(&GOLD_FEE_MULT.pow(tx_count as u16, prec)?, prec)?;
        let silver = SILVER_FEE_MIN.mul(&SILVER_FEE_MULT.pow(tx_count as u16, prec)?, prec)?;

        Some(Balance { gold, silver })
    }

    pub fn get_balance(&self, addr: &PublicKey) -> Balance {
        self.indexer.get_balance(addr).unwrap_or_default()
    }

    pub fn insert_block(&self, block: SignedBlock) -> Result<(), String> {
        self.verify_block(&block, &self.get_chain_head())?;
        for tx in &block.transactions { self.index_tx(tx); }

        let lock = self.store.lock().unwrap();
        let store = &mut lock.borrow_mut();
        store.insert(block);

        Ok(())
    }

    fn verify_block(&self, block: &SignedBlock, prev_block: &SignedBlock) -> Result<(), String> {
        if prev_block.height + 1 != block.height {
            return Err("invalid block height".to_owned())
        } else if !block.verify_tx_merkle_root() {
            return Err("invalid merkle root".to_owned())
        } else if !block.verify_previous_hash(prev_block) {
            return Err("invalid previous hash".to_owned())
        }

        if self.indexer.get_bond(&block.sig_pair.pub_key).is_none() {
            return Err("bond not found".to_owned())
        } else if !block.sig_pair.verify(block.calc_hash().as_ref()) {
            return Err("invalid bond signature".to_owned())
        }

        for tx in &block.transactions {
            if let Err(s) = self.verify_tx(tx) {
                return Err(format!("tx verification failed: {}", s))
            }
        }

        Ok(())
    }

    fn verify_tx(&self, tx: &TxVariant) -> Result<(), String> {
        match tx {
            TxVariant::RewardTx(tx) => {
                if tx.signature_pairs.len() != 0 {
                    return Err("reward transaction must not be signed".to_owned())
                }
            },
            TxVariant::BondTx(tx) => {
                // TODO get address fee
                // TODO validate bond fee is sufficient
                // TODO validate stake amt is greater than 0
            },
            TxVariant::TransferTx(tx) => {
                // TODO get address fee
                // TODO validate "from" has enough balance
            }
        }
        Ok(())
    }

    fn index_tx(&self, tx: &TxVariant) {
        match tx {
            TxVariant::RewardTx(tx) => {
                let mut bal = self.get_balance(&tx.to);
                for r in &tx.rewards {
                    bal.add(r);
                }
                self.indexer.set_balance(&tx.to, &bal);
            },
            TxVariant::BondTx(tx) => {
                let mut bal = self.get_balance(&tx.staker);
                bal.sub(&tx.fee).sub(&tx.bond_fee).sub(&tx.stake_amt);
                self.indexer.set_balance(&tx.staker, &bal);
            },
            TxVariant::TransferTx(tx) => {
                let mut from_bal = self.get_balance(&tx.from);
                let mut to_bal = self.get_balance(&tx.to);

                from_bal.sub(&tx.fee).sub(&tx.amount);
                to_bal.add(&tx.amount);

                self.indexer.set_balance(&tx.from, &from_bal);
                self.indexer.set_balance(&tx.to, &to_bal);
            }
        }
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
}
