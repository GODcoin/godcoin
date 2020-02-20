use log::info;
use parking_lot::Mutex;
use std::{path::Path, sync::Arc};

pub mod block;
pub mod error;
pub mod index;
pub mod receipt;
pub mod skip_flags;
pub mod store;

pub use self::{
    block::*,
    error::*,
    index::{IndexStatus, Indexer, WriteBatch},
    receipt::*,
    store::{BlockStore, ReindexOpts},
};

use crate::{asset::Asset, constants::*, crypto::*, script::*, tx::*};
use skip_flags::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Properties {
    pub height: u64,
    pub owner: Box<TxVariant>,
    pub network_fee: Asset,
    pub token_supply: Asset,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AddressInfo {
    pub net_fee: Asset,
    pub addr_fee: Asset,
    pub balance: Asset,
}

impl AddressInfo {
    pub fn total_fee(&self) -> Option<Asset> {
        self.net_fee.checked_add(self.addr_fee)
    }
}

pub struct Blockchain {
    indexer: Arc<Indexer>,
    store: Mutex<BlockStore>,
}

impl Blockchain {
    ///
    /// Creates a new `Blockchain` with an associated indexer and block log based on the
    /// provided paths.
    ///
    pub fn new(blocklog_loc: &Path, index_loc: &Path) -> Self {
        let indexer = Arc::new(Indexer::new(index_loc));
        let store = BlockStore::new(blocklog_loc, Arc::clone(&indexer));
        Blockchain {
            indexer,
            store: Mutex::new(store),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.store.lock().is_empty()
    }

    pub fn indexer(&self) -> Arc<Indexer> {
        Arc::clone(&self.indexer)
    }

    pub fn index_status(&self) -> IndexStatus {
        self.indexer.index_status()
    }

    pub fn reindex(&self, opts: ReindexOpts) {
        {
            let status = self.indexer.index_status();
            if status != IndexStatus::None {
                panic!("expected index status to be None, got: {:?}", status);
            }
        }
        let mut store = self.store.lock();
        store.reindex_blocks(opts, |batch, block| {
            self.index_block(batch, block);
            if block.height() % 1000 == 0 {
                info!("Indexed block {}", block.height());
            }
        });

        info!("Rebuilding tx expiry index");
        let manager = index::TxManager::new(self.indexer());
        let current_time = crate::get_epoch_time();
        // Iterate in reverse from head to genesis block
        for height in (0..=self.get_chain_height()).rev() {
            let block = store.get(height).unwrap();
            if current_time - block.timestamp() <= TX_MAX_EXPIRY_TIME {
                for receipt in block.receipts() {
                    let data = TxPrecompData::from_tx(&receipt.tx);
                    let expiry = data.tx().expiry();
                    if expiry > current_time {
                        manager.insert(data.txid(), expiry);
                    }
                }
            } else {
                // Break early as all transactions are guaranteed to be expired.
                break;
            }
        }

        info!("Reindexing complete");
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
    pub fn get_owner(&self) -> TxVariant {
        self.indexer
            .get_owner()
            .expect("Failed to retrieve owner from index")
    }

    #[inline]
    pub fn get_chain_height(&self) -> u64 {
        self.indexer.get_chain_height()
    }

    pub fn get_chain_head(&self) -> Arc<Block> {
        let store = self.store.lock();
        let height = store.get_chain_height();
        store.get(height).expect("Failed to get blockchain head")
    }

    pub fn get_block(&self, height: u64) -> Option<Arc<Block>> {
        let store = self.store.lock();
        store.get(height)
    }

    /// Gets a filtered block using the `filter` at the specified `height`. This does not match whether the `filter`
    /// contains an owner address to match block rewards.
    pub fn get_filtered_block(&self, height: u64, filter: &BlockFilter) -> Option<FilteredBlock> {
        let store = self.store.lock();
        let block = store.get(height);

        match block {
            Some(block) => {
                let has_match = if filter.is_empty() {
                    false
                } else {
                    block.receipts().iter().any(|receipt| match &receipt.tx {
                        TxVariant::V0(tx) => match tx {
                            TxVariantV0::OwnerTx(owner_tx) => {
                                let hash = ScriptHash::from(&owner_tx.script);
                                filter.contains(&owner_tx.wallet) || filter.contains(&hash)
                            }
                            TxVariantV0::MintTx(mint_tx) => {
                                let hash = ScriptHash::from(&mint_tx.script);
                                filter.contains(&hash) || filter.contains(&mint_tx.to)
                            }
                            TxVariantV0::TransferTx(transfer_tx) => {
                                if filter.contains(&transfer_tx.from) {
                                    return true;
                                }
                                for entry in &receipt.log {
                                    match entry {
                                        LogEntry::Transfer(to_addr, _) => {
                                            if filter.contains(to_addr) {
                                                return true;
                                            }
                                        }
                                    }
                                }
                                false
                            }
                        },
                    })
                };
                if has_match {
                    Some(FilteredBlock::Block(block))
                } else {
                    let signer = block.signer().unwrap().clone();
                    Some(FilteredBlock::Header((block.header(), signer)))
                }
            }
            None => None,
        }
    }

    pub fn get_balance(&self, addr: &ScriptHash, additional_receipts: &[Receipt]) -> Option<Asset> {
        let mut bal = self.indexer.get_balance(addr).unwrap_or_default();
        for receipt in additional_receipts {
            match &receipt.tx {
                TxVariant::V0(tx) => match tx {
                    TxVariantV0::OwnerTx(_) => {}
                    TxVariantV0::MintTx(tx) => {
                        if &tx.to == addr {
                            bal = bal.checked_add(tx.amount)?;
                        }
                    }
                    TxVariantV0::TransferTx(tx) => {
                        if &tx.from == addr {
                            bal = bal.checked_sub(tx.fee)?;
                            bal = bal.checked_sub(tx.amount)?;
                        }
                        for entry in &receipt.log {
                            match entry {
                                LogEntry::Transfer(to_addr, amount) => {
                                    if to_addr == addr {
                                        bal = bal.checked_add(*amount)?;
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }

        Some(bal)
    }

    pub fn get_address_info(
        &self,
        addr: &ScriptHash,
        additional_receipts: &[Receipt],
    ) -> Option<AddressInfo> {
        let net_fee = self.get_network_fee()?;
        let addr_fee = self.get_address_fee(addr, additional_receipts)?;
        let balance = self.get_balance(addr, additional_receipts)?;
        Some(AddressInfo {
            net_fee,
            addr_fee,
            balance,
        })
    }

    pub fn get_address_fee(
        &self,
        addr: &ScriptHash,
        additional_receipts: &[Receipt],
    ) -> Option<Asset> {
        let mut count = 1;
        let mut delta = 0;

        macro_rules! handle_receipt_match {
            ($receipt:expr) => {
                let has_match = match &$receipt.tx {
                    TxVariant::V0(tx) => match tx {
                        TxVariantV0::OwnerTx(_) => false,
                        TxVariantV0::MintTx(_) => false,
                        TxVariantV0::TransferTx(tx) => &tx.from == addr,
                    },
                };
                if has_match {
                    count += 1;
                    // Reset the delta count when a match is found
                    delta = 0;
                }
            };
        }

        for r in additional_receipts {
            handle_receipt_match!(r);
        }

        for i in (0..=self.get_chain_height()).rev() {
            delta += 1;
            let block = self.get_block(i).unwrap();
            for r in block.receipts() {
                handle_receipt_match!(r);
            }
            if delta == FEE_RESET_WINDOW {
                break;
            }
        }

        GRAEL_FEE_MIN.checked_mul(GRAEL_FEE_MULT.checked_pow(count as u16)?)
    }

    pub fn get_network_fee(&self) -> Option<Asset> {
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

        let mut count: u64 = 1;
        for i in min_height..=max_height {
            count += self.get_block(i).unwrap().receipts().len() as u64;
        }
        count /= NETWORK_FEE_AVG_WINDOW;
        if count > u64::from(u16::max_value()) {
            return None;
        }

        GRAEL_FEE_MIN.checked_mul(GRAEL_FEE_NET_MULT.checked_pow(count as u16)?)
    }

    pub fn insert_block(&self, block: Block) -> Result<(), BlockErr> {
        static SKIP_FLAGS: SkipFlags = SKIP_NONE;
        self.verify_block(&block, &self.get_chain_head(), SKIP_FLAGS)?;
        let mut batch = WriteBatch::new(Arc::clone(&self.indexer));
        self.index_block(&mut batch, &block);
        self.store.lock().insert(&mut batch, block);
        batch.commit();

        Ok(())
    }

    fn verify_block(
        &self,
        block: &Block,
        prev_block: &Block,
        skip_flags: SkipFlags,
    ) -> Result<(), BlockErr> {
        if prev_block.height() + 1 != block.height() {
            return Err(BlockErr::InvalidBlockHeight);
        } else if !block.verify_receipt_root() {
            return Err(BlockErr::InvalidReceiptRoot);
        } else if !block.verify_previous_hash(prev_block) {
            return Err(BlockErr::InvalidPrevHash);
        }

        let block_signer = block.signer().ok_or(BlockErr::InvalidSignature)?;
        match self.get_owner() {
            TxVariant::V0(tx) => match tx {
                TxVariantV0::OwnerTx(owner) => {
                    if block_signer.pub_key != owner.minter {
                        return Err(BlockErr::InvalidSignature);
                    }
                }
                _ => unreachable!(),
            },
        }

        if !block_signer.verify(block.calc_header_hash().as_ref()) {
            return Err(BlockErr::InvalidSignature);
        }

        let block_receipts = block.receipts();
        let len = block_receipts.len();
        for i in 0..len {
            let r = &block_receipts[i];
            let receipts = &block_receipts[0..i];
            if let Err(e) = self.execute_tx(&TxPrecompData::from_tx(&r.tx), receipts, skip_flags) {
                return Err(BlockErr::Tx(e));
            }
        }

        Ok(())
    }

    pub fn execute_tx(
        &self,
        data: &TxPrecompData,
        additional_receipts: &[Receipt],
        _skip_flags: SkipFlags,
    ) -> Result<Vec<LogEntry>, TxErr> {
        macro_rules! check_zero_fee {
            ($asset:expr) => {
                if $asset.amount != 0 {
                    return Err(TxErr::InvalidFeeAmount);
                }
            };
        }

        macro_rules! check_pos_amt {
            ($asset:expr) => {
                if $asset.amount < 0 {
                    return Err(TxErr::InvalidAmount);
                }
            };
        }

        macro_rules! check_suf_bal {
            ($asset:expr) => {
                if $asset.amount < 0 {
                    return Err(TxErr::InvalidAmount);
                }
            };
        }

        let tx = data.tx();

        if tx.sigs().len() > MAX_TX_SIGNATURES {
            return Err(TxErr::TooManySignatures);
        } else if let Some(script) = tx.script() {
            if script.len() > MAX_SCRIPT_BYTE_SIZE {
                return Err(TxErr::TxTooLarge);
            }
        }

        match tx {
            TxVariant::V0(tx) => match tx {
                TxVariantV0::OwnerTx(new_owner) => {
                    check_zero_fee!(tx.fee);

                    match self.get_owner() {
                        TxVariant::V0(tx) => match tx {
                            TxVariantV0::OwnerTx(owner) => {
                                if owner.wallet != (&new_owner.script).into() {
                                    return Err(TxErr::ScriptHashMismatch);
                                }
                            }
                            _ => unreachable!(),
                        },
                    }

                    if let Err(e) = ScriptEngine::new(data, &new_owner.script).eval() {
                        return Err(TxErr::ScriptEval(e));
                    }
                    Ok(vec![])
                }
                TxVariantV0::MintTx(mint_tx) => {
                    check_zero_fee!(tx.fee);
                    check_pos_amt!(mint_tx.amount);

                    match self.get_owner() {
                        TxVariant::V0(tx) => match tx {
                            TxVariantV0::OwnerTx(owner) => {
                                if owner.wallet != (&mint_tx.script).into() {
                                    return Err(TxErr::ScriptHashMismatch);
                                }
                            }
                            _ => unreachable!(),
                        },
                    }

                    if let Err(e) = ScriptEngine::new(data, &mint_tx.script).eval() {
                        return Err(TxErr::ScriptEval(e));
                    }

                    // Sanity check to ensure too many new coins can't be minted
                    self.get_balance(&mint_tx.to, additional_receipts)
                        .ok_or(TxErr::Arithmetic)?
                        .checked_add(mint_tx.amount)
                        .ok_or(TxErr::Arithmetic)?;

                    self.indexer
                        .get_token_supply()
                        .checked_add(mint_tx.amount)
                        .ok_or(TxErr::Arithmetic)?;

                    Ok(vec![])
                }
                TxVariantV0::TransferTx(transfer) => {
                    if transfer.memo.len() > MAX_MEMO_BYTE_SIZE {
                        return Err(TxErr::TxTooLarge);
                    }
                    check_pos_amt!(transfer.amount);

                    let info = self
                        .get_address_info(&transfer.from, additional_receipts)
                        .ok_or(TxErr::Arithmetic)?;
                    let total_fee = info.total_fee().ok_or(TxErr::Arithmetic)?;
                    if tx.fee < total_fee {
                        return Err(TxErr::InvalidFeeAmount);
                    } else if transfer.from != (&transfer.script).into() {
                        return Err(TxErr::ScriptHashMismatch);
                    }

                    let bal = info
                        .balance
                        .checked_sub(transfer.fee)
                        .ok_or(TxErr::Arithmetic)?
                        .checked_sub(transfer.amount)
                        .ok_or(TxErr::Arithmetic)?;
                    check_suf_bal!(bal);

                    let log = ScriptEngine::new(data, &transfer.script)
                        .eval()
                        .map_err(|e| TxErr::ScriptEval(e))?;
                    Ok(log)
                }
            },
        }
    }

    fn index_block(&self, batch: &mut WriteBatch, block: &Block) {
        for r in block.receipts() {
            Self::index_receipt(batch, r);
        }
        let owner_tx = match batch.get_owner() {
            Some(tx) => tx.clone(),
            None => self.get_owner(),
        };
        match owner_tx {
            TxVariant::V0(TxVariantV0::OwnerTx(tx)) => {
                batch.add_bal(&tx.wallet, block.rewards());
            }
            _ => panic!("expected owner transaction"),
        };
    }

    fn index_receipt(batch: &mut WriteBatch, receipt: &Receipt) {
        let tx = &receipt.tx;
        match tx {
            TxVariant::V0(var) => match var {
                TxVariantV0::OwnerTx(_) => {
                    batch.set_owner(tx.clone());
                }
                TxVariantV0::MintTx(tx) => {
                    batch.add_token_supply(tx.amount);
                    batch.add_bal(&tx.to, tx.amount);
                }
                TxVariantV0::TransferTx(tx) => {
                    batch.sub_bal(&tx.from, tx.fee.checked_add(tx.amount).unwrap());
                    for entry in &receipt.log {
                        match entry {
                            LogEntry::Transfer(to_addr, amount) => batch.add_bal(to_addr, *amount),
                        }
                    }
                }
            },
        }
    }

    pub fn create_genesis_block(&self, minter_key: KeyPair) -> GenesisBlockInfo {
        let info = GenesisBlockInfo::new(minter_key);
        let timestamp = crate::get_epoch_time();

        let owner_tx = TxVariant::V0(TxVariantV0::OwnerTx(OwnerTx {
            base: Tx {
                nonce: 0,
                expiry: timestamp + 1,
                fee: Asset::default(),
                signature_pairs: Vec::new(),
            },
            minter: info.minter_key.0.clone(),
            wallet: (&info.script).into(),
            script: Builder::new()
                .push(FnBuilder::new(0, OpFrame::OpDefine(vec![])).push(OpFrame::False))
                .build()
                .unwrap(),
        }));

        let receipts = vec![Receipt {
            tx: owner_tx.clone(),
            log: vec![],
        }];
        let receipt_root = calc_receipt_root(&receipts);

        let mut block = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                height: 0,
                previous_hash: Digest::from_slice(&[0u8; 32]).unwrap(),
                receipt_root,
                timestamp,
            },
            signer: None,
            rewards: Asset::default(),
            receipts,
        });
        block.sign(&info.minter_key);

        let mut batch = WriteBatch::new(Arc::clone(&self.indexer));
        self.store.lock().insert_genesis(&mut batch, block);
        batch.set_owner(owner_tx);
        batch.commit();
        self.indexer.set_index_status(IndexStatus::Complete);

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
            KeyPair::gen(),
            KeyPair::gen(),
            KeyPair::gen(),
            KeyPair::gen(),
        ];

        let script = Builder::new()
            .push(
                // The purpose of this function is to be used for minting transactions
                FnBuilder::new(0, OpFrame::OpDefine(vec![]))
                    .push(OpFrame::PubKey(wallet_keys[0].0.clone()))
                    .push(OpFrame::PubKey(wallet_keys[1].0.clone()))
                    .push(OpFrame::PubKey(wallet_keys[2].0.clone()))
                    .push(OpFrame::PubKey(wallet_keys[3].0.clone()))
                    .push(OpFrame::OpCheckMultiSig(2, 4)),
            )
            .push(
                // Standard transfer function
                FnBuilder::new(1, OpFrame::OpDefine(vec![Arg::ScriptHash, Arg::Asset]))
                    .push(OpFrame::PubKey(wallet_keys[0].0.clone()))
                    .push(OpFrame::PubKey(wallet_keys[1].0.clone()))
                    .push(OpFrame::PubKey(wallet_keys[2].0.clone()))
                    .push(OpFrame::PubKey(wallet_keys[3].0.clone()))
                    .push(OpFrame::OpCheckMultiSigFastFail(2, 4))
                    .push(OpFrame::OpTransfer)
                    .push(OpFrame::True),
            )
            .build()
            .unwrap();

        Self {
            minter_key,
            wallet_keys,
            script,
        }
    }
}
