use parking_lot::Mutex;
use std::{path::*, sync::Arc};

pub mod block;
pub mod index;
pub mod store;
pub mod verify;

pub use self::{block::*, index::Indexer, store::BlockStore, verify::TxErr};

use crate::{
    asset::{self, Balance},
    crypto::*,
    script::*,
    tx::*,
};

#[derive(Clone, Debug)]
pub struct Properties {
    pub height: u64,
    pub owner: Box<OwnerTx>,
    pub token_supply: Balance,
    pub network_fee: Balance,
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
        addr_fee.add_bal(&net_fee)?;
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
                        bal.add_bal(&tx.amount)?;
                    }
                }
                TxVariant::RewardTx(tx) => {
                    if &tx.to == hash {
                        bal.add_bal(&tx.rewards)?;
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

    pub fn insert_block(&self, block: SignedBlock) -> Result<(), verify::BlockError> {
        static CONFIG: verify::Config = verify::Config { skip_reward: true };
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
        config: verify::Config,
    ) -> Result<(), verify::BlockError> {
        if prev_block.height + 1 != block.height {
            return Err(verify::BlockError::InvalidBlockHeight);
        } else if !block.verify_tx_merkle_root() {
            return Err(verify::BlockError::InvalidMerkleRoot);
        } else if !block.verify_previous_hash(prev_block) {
            return Err(verify::BlockError::InvalidPrevHash);
        }

        let owner = self.get_owner();
        if !block.sig_pair.verify(block.calc_hash().as_ref()) {
            return Err(verify::BlockError::InvalidHash);
        } else if block.sig_pair.pub_key != owner.minter {
            return Err(verify::BlockError::InvalidSignature);
        }

        let len = block.transactions.len();
        for i in 0..len {
            let tx = &block.transactions[i];
            let txs = &block.transactions[0..i];
            if let Err(e) = self.verify_tx(tx, txs, config) {
                return Err(verify::BlockError::Tx(e));
            }
        }

        Ok(())
    }

    pub fn verify_tx(
        &self,
        tx: &TxVariant,
        additional_txs: &[TxVariant],
        config: verify::Config,
    ) -> Result<(), verify::TxErr> {
        macro_rules! check_suf_bal {
            ($asset:expr) => {
                if $asset.amount < 0 {
                    return Err(verify::TxErr::InsufficientBalance);
                }
            };
        }

        if !(tx.tx_type == TxType::OWNER || tx.tx_type == TxType::MINT) {
            if tx.fee.amount < 0 {
                return Err(verify::TxErr::InsufficientFeeAmount);
            }
        }

        match tx {
            TxVariant::OwnerTx(new_owner) => {
                let owner = self.get_owner();
                if owner.wallet != (&new_owner.script).into() {
                    return Err(verify::TxErr::ScriptHashMismatch);
                }

                let success = ScriptEngine::checked_new(tx, &new_owner.script)
                    .map_err(TxErr::from)?
                    .eval()
                    .map_err(verify::TxErr::ScriptEval)?;
                if !success {
                    return Err(verify::TxErr::ScriptRetFalse);
                }
            }
            TxVariant::MintTx(mint_tx) => {
                let owner = self.get_owner();
                if owner.wallet != (&mint_tx.script).into() {
                    return Err(verify::TxErr::ScriptHashMismatch);
                }
                let success = ScriptEngine::checked_new(tx, &mint_tx.script)
                    .map_err(TxErr::from)?
                    .eval()
                    .map_err(verify::TxErr::ScriptEval)?;
                if !success {
                    return Err(verify::TxErr::ScriptRetFalse);
                }

                // Sanity check to ensure too many new coins can't be minted
                self.get_balance_with_txs(&mint_tx.to, additional_txs)
                    .ok_or(verify::TxErr::Arithmetic)?
                    .add_bal(&mint_tx.amount)
                    .ok_or(verify::TxErr::Arithmetic)?;

                self.indexer
                    .get_token_supply()
                    .add_bal(&mint_tx.amount)
                    .ok_or(verify::TxErr::Arithmetic)?;
            }
            TxVariant::RewardTx(tx) => {
                if !config.skip_reward {
                    return Err(verify::TxErr::TxProhibited);
                } else if !tx.signature_pairs.is_empty() {
                    // Reward transactions are internally generated, thus should panic on failure
                    panic!("reward transaction must not be signed");
                }
            }
            TxVariant::TransferTx(transfer) => {
                if transfer.fee.symbol != transfer.amount.symbol {
                    return Err(verify::TxErr::SymbolMismatch);
                } else if transfer.from != (&transfer.script).into() {
                    return Err(verify::TxErr::ScriptHashMismatch);
                }

                let success = ScriptEngine::checked_new(tx, &transfer.script)
                    .map_err(TxErr::from)?
                    .eval()
                    .map_err(verify::TxErr::ScriptEval)?;
                if !success {
                    return Err(verify::TxErr::ScriptRetFalse);
                }

                let mut bal = self
                    .get_balance_with_txs(&transfer.from, additional_txs)
                    .ok_or(verify::TxErr::Arithmetic)?;
                bal.sub(&transfer.fee)
                    .ok_or(verify::TxErr::Arithmetic)?
                    .sub(&transfer.amount)
                    .ok_or(verify::TxErr::Arithmetic)?;
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
                supply.add_bal(&tx.amount).unwrap();
                self.indexer.set_token_supply(&supply);

                let mut bal = self.get_balance(&tx.to);
                bal.add_bal(&tx.amount).unwrap();
                self.indexer.set_balance(&tx.to, &bal);
            }
            TxVariant::RewardTx(tx) => {
                let mut bal = self.get_balance(&tx.to);
                bal.add_bal(&tx.rewards).unwrap();
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
