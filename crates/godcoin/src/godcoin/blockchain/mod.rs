use parking_lot::Mutex;
use std::path::*;
use std::sync::Arc;

pub mod block;
pub mod index;
pub mod store;

pub use self::block::*;
pub use self::index::Indexer;
pub use self::store::BlockStore;

use crate::asset::{self, Balance};
use crate::crypto::*;
use crate::script::*;
use crate::tx::*;

#[derive(Clone, Debug)]
pub struct Properties {
    pub height: u64,
    pub owner: Box<OwnerTx>,
    pub token_supply: Balance,
    pub network_fee: Balance,
}

#[derive(Copy, Clone, Debug)]
pub struct VerifyConfig {
    skip_reward: bool,
}

impl VerifyConfig {
    pub const fn strict() -> Self {
        VerifyConfig { skip_reward: false }
    }
}

pub struct Blockchain {
    indexer: Arc<Indexer>,
    store: Mutex<BlockStore>,
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
            store: Mutex::new(store),
        }
    }

    pub fn get_properties(&self) -> Properties {
        Properties {
            height: self.get_chain_height(),
            owner: Box::new(self.get_owner()),
            token_supply: self.indexer.get_token_supply(),
            network_fee: self
                .get_network_fee()
                .expect("unexpected error retrieving network fee"),
        }
    }

    #[inline]
    pub fn get_owner(&self) -> OwnerTx {
        self.indexer
            .get_owner()
            .expect("Failed to retrieve owner from index")
    }

    #[inline]
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

    pub fn get_total_fee(&self, hash: &ScriptHash) -> Option<Balance> {
        let net_fee = self.get_network_fee()?;
        let mut addr_fee = self.get_address_fee(hash)?;
        addr_fee.add(net_fee.gold())?;
        addr_fee.add(net_fee.silver())?;
        Some(addr_fee)
    }

    pub fn get_address_fee(&self, hash: &ScriptHash) -> Option<Balance> {
        use crate::constants::*;

        let mut tx_count = 1;
        let head = self.get_chain_height();
        for (delta, i) in (0..=head).rev().enumerate() {
            let block = self.get_block(i).unwrap();
            for tx in &block.transactions {
                let has_match = match tx {
                    TxVariant::OwnerTx(_) => false,
                    TxVariant::MintTx(_) => false,
                    TxVariant::RewardTx(_) => false,
                    TxVariant::TransferTx(tx) => &tx.from == hash,
                };
                if has_match {
                    tx_count += 1;
                }
            }
            if delta + 1 == FEE_RESET_WINDOW {
                break;
            }
        }

        let prec = asset::MAX_PRECISION;
        let gold = GOLD_FEE_MIN.mul(&GOLD_FEE_MULT.pow(tx_count as u16, prec)?, prec)?;
        let silver = SILVER_FEE_MIN.mul(&SILVER_FEE_MULT.pow(tx_count as u16, prec)?, prec)?;
        Balance::from(gold, silver)
    }

    pub fn get_network_fee(&self) -> Option<Balance> {
        // The network fee adjusts every 5 blocks so that users have a bigger time
        // frame to confirm the fee they want to spend without suddenly changing.
        use crate::constants::*;
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
        tx_count /= NETWORK_FEE_AVG_WINDOW;
        if tx_count > u64::from(u16::max_value()) {
            return None;
        }

        let prec = asset::MAX_PRECISION;
        let gold = GOLD_FEE_MIN.mul(&GOLD_FEE_NET_MULT.pow(tx_count as u16, prec)?, prec)?;
        let silver = SILVER_FEE_MIN.mul(&SILVER_FEE_NET_MULT.pow(tx_count as u16, prec)?, prec)?;

        Balance::from(gold, silver)
    }

    pub fn get_balance(&self, hash: &ScriptHash) -> Balance {
        self.indexer.get_balance(hash).unwrap_or_default()
    }

    pub fn get_balance_with_txs(&self, hash: &ScriptHash, txs: &[TxVariant]) -> Option<Balance> {
        let mut bal = self.indexer.get_balance(hash).unwrap_or_default();
        for tx in txs {
            match tx {
                TxVariant::OwnerTx(_) => {}
                TxVariant::MintTx(tx) => {
                    if &tx.to == hash {
                        bal.add(tx.amount.gold())?;
                        bal.add(tx.amount.silver())?;
                    }
                }
                TxVariant::RewardTx(tx) => {
                    if &tx.to == hash {
                        bal.add(tx.rewards.gold())?;
                        bal.add(tx.rewards.silver())?;
                    }
                }
                TxVariant::TransferTx(tx) => {
                    if &tx.from == hash {
                        bal.sub(&tx.fee)?;
                        bal.sub(&tx.amount)?;
                    } else if &tx.to == hash {
                        bal.add(&tx.amount)?;
                    }
                }
            }
        }

        Some(bal)
    }

    pub fn insert_block(&self, block: SignedBlock) -> Result<(), String> {
        static CONFIG: VerifyConfig = VerifyConfig { skip_reward: true };
        self.verify_block(&block, &self.get_chain_head(), CONFIG)?;
        for tx in &block.transactions {
            self.index_tx(tx);
        }
        self.store.lock().insert(block);

        Ok(())
    }

    fn verify_block(
        &self,
        block: &SignedBlock,
        prev_block: &SignedBlock,
        config: VerifyConfig,
    ) -> Result<(), String> {
        if prev_block.height + 1 != block.height {
            return Err("invalid block height".to_owned());
        } else if !block.verify_tx_merkle_root() {
            return Err("invalid merkle root".to_owned());
        } else if !block.verify_previous_hash(prev_block) {
            return Err("invalid previous hash".to_owned());
        }

        let owner = self.get_owner();
        if !block.sig_pair.verify(block.calc_hash().as_ref()) {
            return Err("invalid block signature".to_owned());
        } else if block.sig_pair.pub_key != owner.minter {
            return Err("invalid owner signature".to_owned());
        }

        let len = block.transactions.len();
        for i in 0..len {
            let tx = &block.transactions[i];
            let txs = &block.transactions[0..i];
            if let Err(s) = self.verify_tx(tx, txs, config) {
                return Err(format!("tx verification failed: {}", s));
            }
        }

        Ok(())
    }

    pub fn verify_tx(
        &self,
        tx: &TxVariant,
        additional_txs: &[TxVariant],
        config: VerifyConfig,
    ) -> Result<(), String> {
        macro_rules! check_amt {
            ($asset:expr, $name:expr) => {
                if $asset.amount < 0 {
                    return Err(format!("{} must be greater than 0", $name));
                }
            };
        }

        macro_rules! check_suf_bal {
            ($asset:expr) => {
                if $asset.amount < 0 {
                    return Err("insufficient balance".to_owned());
                }
            };
        }

        if !(tx.tx_type == TxType::OWNER || tx.tx_type == TxType::MINT) {
            check_amt!(tx.fee, "fee");
        }

        match tx {
            TxVariant::OwnerTx(new_owner) => {
                let owner = self.get_owner();
                if owner.wallet != (&new_owner.script).into() {
                    return Err("script hash does not match previous wallet address".to_owned());
                }

                let success = ScriptEngine::checked_new(tx, &new_owner.script)
                    .ok_or_else(|| "failed to initialize script engine")?
                    .eval()
                    .map_err(|e| format!("{}: {:?}", e.pos, e.err))?;
                if !success {
                    return Err("script returned false".to_owned());
                }
            }
            TxVariant::MintTx(mint_tx) => {
                let owner = self.get_owner();
                if owner.wallet != (&mint_tx.script).into() {
                    return Err("script hash does not match current wallet address".to_owned());
                }
                let success = ScriptEngine::checked_new(tx, &mint_tx.script)
                    .ok_or_else(|| "failed to initialize script engine")?
                    .eval()
                    .map_err(|e| format!("{}: {:?}", e.pos, e.err))?;
                if !success {
                    return Err("script returned false".to_owned());
                }
            }
            TxVariant::RewardTx(tx) => {
                if !config.skip_reward {
                    return Err("reward transactions are prohibited".to_owned());
                } else if !tx.signature_pairs.is_empty() {
                    return Err("reward transaction must not be signed".to_owned());
                }
            }
            TxVariant::TransferTx(transfer) => {
                if transfer.fee.symbol != transfer.amount.symbol {
                    return Err("symbol mismatch between fee and amount".to_owned());
                } else if transfer.from != (&transfer.script).into() {
                    return Err("from and script hash mismatch".to_owned());
                }

                let success = ScriptEngine::checked_new(tx, &transfer.script)
                    .ok_or_else(|| "failed to initialize script engine")?
                    .eval()
                    .map_err(|e| format!("{}: {:?}", e.pos, e.err))?;
                if !success {
                    return Err("script returned false".to_owned());
                }

                let mut bal = self
                    .get_balance_with_txs(&transfer.from, additional_txs)
                    .ok_or_else(|| "failed to get balance")?;
                bal.sub(&transfer.fee)
                    .ok_or("failed to subtract fee")?
                    .sub(&transfer.amount)
                    .ok_or("failed to subtract amount")?;
                check_suf_bal!(bal.gold());
                check_suf_bal!(bal.silver());
            }
        }
        Ok(())
    }

    fn index_tx(&self, tx: &TxVariant) {
        match tx {
            TxVariant::OwnerTx(tx) => {
                self.indexer.set_owner(tx);
            }
            TxVariant::MintTx(tx) => {
                let mut supply = self.indexer.get_token_supply();
                supply.add(tx.amount.gold()).unwrap();
                supply.add(tx.amount.silver()).unwrap();
                self.indexer.set_token_supply(&supply);
            }
            TxVariant::RewardTx(tx) => {
                let mut bal = self.get_balance(&tx.to);
                bal.add(tx.rewards.gold()).unwrap();
                bal.add(tx.rewards.silver()).unwrap();
                self.indexer.set_balance(&tx.to, &bal);
            }
            TxVariant::TransferTx(tx) => {
                let mut from_bal = self.get_balance(&tx.from);
                let mut to_bal = self.get_balance(&tx.to);

                from_bal.sub(&tx.fee).unwrap().sub(&tx.amount).unwrap();
                to_bal.add(&tx.amount).unwrap();

                self.indexer.set_balance(&tx.from, &from_bal);
                self.indexer.set_balance(&tx.to, &to_bal);
            }
        }
    }

    pub fn create_genesis_block(&self, minter_key: KeyPair) -> GenesisBlockInfo {
        use crate::crypto::Digest;
        use std::time::{SystemTime, UNIX_EPOCH};

        let info = GenesisBlockInfo::new(minter_key);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let owner_tx = OwnerTx {
            base: Tx {
                tx_type: TxType::OWNER,
                fee: "0 GOLD".parse().unwrap(),
                timestamp,
                signature_pairs: Vec::new(),
            },
            minter: info.minter_key.0.clone(),
            wallet: (&info.script).into(),
            script: Builder::new().push(OpFrame::False).build(),
        };

        let block = (Block {
            height: 0,
            previous_hash: Digest::from_slice(&[0u8; 32]).unwrap(),
            tx_merkle_root: Digest::from_slice(&[0u8; 32]).unwrap(),
            timestamp,
            transactions: vec![TxVariant::OwnerTx(owner_tx.clone())],
        })
        .sign(&info.minter_key);

        self.store.lock().insert_genesis(block);
        self.indexer.set_owner(&owner_tx);

        info
    }
}

pub struct GenesisBlockInfo {
    pub minter_key: KeyPair,
    pub wallet_keys: [KeyPair; 4],
    pub script: Script,
}

impl GenesisBlockInfo {
    pub fn new(minter_key: KeyPair) -> Self {
        let wallet_keys = [
            KeyPair::gen_keypair(),
            KeyPair::gen_keypair(),
            KeyPair::gen_keypair(),
            KeyPair::gen_keypair(),
        ];

        let script = Builder::new()
            .push(OpFrame::PubKey(minter_key.0.clone()))
            .push(OpFrame::OpCheckSigFastFail)
            .push(OpFrame::PubKey(wallet_keys[0].0.clone()))
            .push(OpFrame::PubKey(wallet_keys[1].0.clone()))
            .push(OpFrame::PubKey(wallet_keys[2].0.clone()))
            .push(OpFrame::PubKey(wallet_keys[3].0.clone()))
            .push(OpFrame::OpCheckMultiSig(2, 4))
            .build();

        Self {
            minter_key,
            wallet_keys,
            script,
        }
    }
}
