use std::time::{SystemTime, UNIX_EPOCH};
use parking_lot::Mutex;
use std::str::FromStr;
use std::sync::Arc;
use std::path::*;

pub mod block;
pub mod index;
pub mod store;

pub use self::store::BlockStore;
pub use self::index::Indexer;
pub use self::block::*;

use asset::{self, Asset, AssetSymbol, Balance, EMPTY_GOLD};
use constants;
use crypto::*;
use tx::*;

pub struct Blockchain {
    indexer: Arc<Indexer>,
    store: Mutex<BlockStore>
}

#[derive(Clone, Debug)]
pub struct Properties {
    pub height: u64,
    pub token_supply: Balance,
    pub network_fee: Balance
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
            store: Mutex::new(store)
        }
    }

    pub fn get_properties(&self) -> Properties {
        Properties {
            height: self.get_chain_height(),
            token_supply: self.indexer.get_token_supply(),
            network_fee: self.get_network_fee().expect("unexpected error retrieving network fee")
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
        let store = self.store.lock();
        let height = store.get_chain_height();
        store.get(height).expect("Failed to get blockchain head")
    }

    pub fn get_block(&self, height: u64) -> Option<Arc<SignedBlock>> {
        let store = self.store.lock();
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
        for i in (0..=head).rev() {
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
        for i in min_height..=max_height {
            tx_count += self.get_block(i).unwrap().transactions.len() as u64;
        }
        if tx_count > u64::from(u16::max_value()) { return None }

        let prec = asset::MAX_PRECISION;
        let gold = GOLD_FEE_MIN.mul(&GOLD_FEE_NET_MULT.pow(tx_count as u16, prec)?, prec)?;
        let silver = SILVER_FEE_MIN.mul(&SILVER_FEE_NET_MULT.pow(tx_count as u16, prec)?, prec)?;

        Some(Balance { gold, silver })
    }

    pub fn get_balance(&self, addr: &PublicKey) -> Balance {
        self.indexer.get_balance(addr).unwrap_or_default()
    }

    pub fn get_balance_with_txs(&self, addr: &PublicKey, txs: &[TxVariant]) -> Option<Balance> {
        let mut bal = self.indexer.get_balance(addr).unwrap_or_default();
        for tx in txs {
            match tx {
                TxVariant::RewardTx(tx) => {
                    if &tx.to == addr {
                        for reward in &tx.rewards {
                            bal.add(&reward)?;
                        }
                    }
                },
                TxVariant::BondTx(tx) => {
                    if &tx.staker == addr {
                        bal.sub(&tx.fee)?;
                        bal.sub(&tx.bond_fee)?;
                        bal.sub(&tx.stake_amt)?;
                    }
                },
                TxVariant::TransferTx(tx) => {
                    if &tx.from == addr {
                        bal.sub(&tx.fee)?;
                        bal.sub(&tx.amount)?;
                    } else if &tx.to == addr {
                        bal.add(&tx.amount)?;
                    }
                }
            }
        }

        Some(bal)
    }

    pub fn insert_block(&self, block: SignedBlock) -> Result<(), String> {
        self.verify_block(&block, &self.get_chain_head())?;
        for tx in &block.transactions { self.index_tx(tx); }
        self.store.lock().insert(block);

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

        let len = block.transactions.len();
        for i in 0..len {
            let tx = &block.transactions[i];
            let txs = &block.transactions[0..i];
            if let Err(s) = self.verify_tx(tx, txs) {
                return Err(format!("tx verification failed: {}", s))
            }
        }

        Ok(())
    }

    pub fn verify_tx(&self, tx: &TxVariant, additional_txs: &[TxVariant]) -> Result<(), String> {
        macro_rules! check_amt {
            ($asset:expr, $name:expr) => {
                if $asset.amount < 0 { return Err(format!("{} must be greater than 0", $name)) }
            }
        }

        macro_rules! check_suf_bal {
            ($asset:expr) => {
                if $asset.amount < 0 { return Err("insufficient balance".to_owned()) }
            }
        }

        check_amt!(tx.fee, "fee");
        if tx.tx_type != TxType::REWARD {
            if tx.signature_pairs.len() != 1 {
                return Err("must have 1 signature".to_owned())
            }

            let mut v = Vec::with_capacity(4096);
            tx.encode(&mut v);
            if !tx.signature_pairs[0].verify(&v) {
                return Err("signature verification failed".to_owned())
            }
        }

        match tx {
            TxVariant::RewardTx(tx) => {
                if !tx.signature_pairs.is_empty() {
                    return Err("reward transaction must not be signed".to_owned())
                }
            },
            TxVariant::BondTx(tx) => {
                if !(tx.bond_fee.symbol == AssetSymbol::GOLD
                        && tx.stake_amt.symbol == AssetSymbol::GOLD
                        && tx.fee.symbol == AssetSymbol::GOLD) {
                    return Err("fees and stake amount must be in gold".to_owned())
                }
                check_amt!(&tx.bond_fee, "bond_fee");
                check_amt!(&tx.stake_amt, "stake_amt");

                if tx.bond_fee.lt(&constants::BOND_FEE).unwrap() {
                    return Err("bond_fee is too small".to_owned())
                }

                let mut bal = self.get_balance_with_txs(&tx.staker, additional_txs).ok_or_else(|| {
                    "failed to get balance"
                })?;
                bal.sub(&tx.fee).ok_or("failed to subtract fee")?
                    .sub(&tx.bond_fee).ok_or("failed to subtract bond_fee")?
                    .sub(&tx.stake_amt).ok_or("failed to subtract stake_amt")?;
                check_suf_bal!(bal.gold);
            },
            TxVariant::TransferTx(tx) => {
                if tx.fee.symbol != tx.amount.symbol {
                    return Err("symbol mismatch between fee and amount".to_owned())
                }
                let mut bal = self.get_balance_with_txs(&tx.from, additional_txs).ok_or_else(|| {
                    "failed to get balance"
                })?;
                bal.sub(&tx.fee).ok_or("failed to subtract fee")?
                    .sub(&tx.amount).ok_or("failed to subtract amount")?;
                check_suf_bal!(&bal.gold);
                check_suf_bal!(&bal.silver);
            }
        }
        Ok(())
    }

    fn index_tx(&self, tx: &TxVariant) {
        match tx {
            TxVariant::RewardTx(tx) => {
                let mut bal = self.get_balance(&tx.to);
                let mut supply = self.indexer.get_token_supply();
                for r in &tx.rewards {
                    bal.add(r).unwrap();
                    supply.add(r).unwrap();
                }
                self.indexer.set_balance(&tx.to, &bal);
                self.indexer.set_token_supply(&supply);
            },
            TxVariant::BondTx(tx) => {
                let mut bal = self.get_balance(&tx.staker);
                bal.sub(&tx.fee).unwrap()
                    .sub(&tx.bond_fee).unwrap()
                    .sub(&tx.stake_amt).unwrap();
                self.indexer.set_balance(&tx.staker, &bal);
                self.indexer.set_bond(tx);

                let mut supply = self.indexer.get_token_supply();
                supply.sub(&tx.bond_fee).unwrap();
                self.indexer.set_token_supply(&supply);
            },
            TxVariant::TransferTx(tx) => {
                let mut from_bal = self.get_balance(&tx.from);
                let mut to_bal = self.get_balance(&tx.to);

                from_bal.sub(&tx.fee).unwrap()
                        .sub(&tx.amount).unwrap();
                to_bal.add(&tx.amount).unwrap();

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
            let mut vec = Vec::with_capacity(2);
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

        self.store.lock().insert_genesis(block);
        self.indexer.set_bond(&bond_tx);
    }
}
