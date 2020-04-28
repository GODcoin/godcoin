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

use crate::{
    account::{Account, AccountId, Permissions},
    asset::Asset,
    constants::*,
    crypto::*,
    script::*,
    tx::*,
};
use skip_flags::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Properties {
    pub height: u64,
    pub owner: Box<TxVariant>,
    pub network_fee: Asset,
    pub token_supply: Asset,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccountInfo {
    pub account: Account,
    pub net_fee: Asset,
    pub account_fee: Asset,
}

impl AccountInfo {
    pub fn total_fee(&self) -> Option<Asset> {
        self.net_fee.checked_add(self.account_fee)
    }
}

#[derive(Debug)]
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

    #[inline]
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
        if store.get_chain_height() == 0 {
            // Attempt to read the raw block stored at byte position 0, which must _always_ be the
            // genesis block. Then, we find the owner wallet account creation and forcibly index it.
            // This will prevent the receipt index process from choking when the creation account is
            // non-existent since the genesis block is the beginning of the chain.
            if let Ok(genesis_block) = store.raw_read_from_disk(0) {
                let receipts = genesis_block.receipts();
                // Two transactions: the first is the creation of the owner wallet, and the second
                // is the configuration of the owner transaction.
                assert_eq!(receipts.len(), 2);
                for r in receipts {
                    match &r.tx {
                        TxVariant::V0(tx) => {
                            if let TxVariantV0::CreateAccountTx(tx) = tx {
                                let mut batch = WriteBatch::new(self.indexer());
                                batch.insert_or_update_account(tx.account.clone());
                                batch.commit();
                            }
                        }
                    }
                }
            }
        }
        store.reindex_blocks(opts, |batch, block| {
            self.index_block(batch, block);
            if block.height() % 1000 == 0 {
                info!("Indexed block {}", block.height());
            }
        });

        info!("Rebuilding tx expiry index");
        let indexer = self.indexer();
        let current_time = crate::get_epoch_time();
        // Iterate in reverse from head to genesis block
        for height in (0..=self.get_chain_height()).rev() {
            let block = store.get(height).unwrap();
            if current_time - block.timestamp() <= TX_MAX_EXPIRY_TIME {
                for receipt in block.receipts() {
                    let data = TxPrecompData::from_tx(&receipt.tx);
                    let expiry = data.tx().expiry();
                    if expiry > current_time {
                        indexer.insert_txid(data.txid(), expiry);
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

    /// Gets a filtered block using the `filter` at the specified `height`. This does not match
    /// whether the `filter` contains an owner account to match block rewards.
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
                            TxVariantV0::OwnerTx(owner_tx) => filter.contains(&owner_tx.wallet),
                            TxVariantV0::MintTx(mint_tx) => filter.contains(&mint_tx.to),
                            TxVariantV0::CreateAccountTx(create_acc_tx) => {
                                filter.contains(&create_acc_tx.creator)
                                    || filter.contains(&create_acc_tx.account.id)
                            }
                            TxVariantV0::UpdateAccountTx(update_acc_tx) => {
                                filter.contains(&update_acc_tx.account_id)
                            }
                            TxVariantV0::TransferTx(transfer_tx) => {
                                if filter.contains(&transfer_tx.from) {
                                    return true;
                                }
                                for entry in &receipt.log {
                                    match entry {
                                        LogEntry::Transfer(to_acc, _) => {
                                            if filter.contains(to_acc) {
                                                return true;
                                            }
                                        }
                                        LogEntry::Destroy(to_acc) => {
                                            if filter.contains(to_acc) {
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

    pub fn get_account(&self, id: AccountId, additional_receipts: &[Receipt]) -> Option<Account> {
        let mut acc = self.indexer.get_account(id)?;
        // This must perform the same actions as when a receipt is indexed. See `fn index_receipt`
        for receipt in additional_receipts {
            match &receipt.tx {
                TxVariant::V0(tx) => match tx {
                    TxVariantV0::OwnerTx(_) => {}
                    TxVariantV0::MintTx(tx) => {
                        if tx.to == id {
                            acc.balance = acc.balance.checked_add(tx.amount)?;
                        }
                    }
                    TxVariantV0::CreateAccountTx(tx) => {
                        if tx.creator == id {
                            acc.balance = acc
                                .balance
                                .checked_sub(tx.account.balance)?
                                .checked_sub(tx.fee)?;
                        }
                    }
                    TxVariantV0::UpdateAccountTx(tx) => {
                        if tx.account_id == id {
                            acc.balance = acc.balance.checked_sub(tx.fee)?;
                        }
                        if let Some(script) = &tx.new_script {
                            acc.script = script.clone();
                        }
                        if let Some(perms) = &tx.new_permissions {
                            acc.permissions = perms.clone();
                        }
                    }
                    TxVariantV0::TransferTx(tx) => {
                        if tx.from == id {
                            acc.balance =
                                acc.balance.checked_sub(tx.fee)?.checked_sub(tx.amount)?;
                        }
                        for entry in &receipt.log {
                            match entry {
                                LogEntry::Transfer(to_acc, amount) => {
                                    if *to_acc == id {
                                        acc.balance = acc.balance.checked_add(*amount)?;
                                    }
                                }
                                LogEntry::Destroy(_to_acc) => {
                                    if tx.from == id {
                                        acc.destroyed = true;
                                        acc.balance = Asset::new(0);
                                    }
                                    // The receiving account is not allowed to see the funds until a
                                    // block is produced. The reason for this is that we cannot
                                    // account for the balance of `tx.from` until *after* the
                                    // destroyed account gets indexed. The exception to this is if
                                    // the current transaction did not use all the funds, a transfer
                                    // log entry will be created for the receiving account, allowing
                                    // the user to see those funds immediately.
                                }
                            }
                        }
                    }
                },
            }
        }

        Some(acc)
    }

    pub fn get_account_info(
        &self,
        id: AccountId,
        additional_receipts: &[Receipt],
    ) -> Option<AccountInfo> {
        let account = self.get_account(id, additional_receipts)?;
        let net_fee = self.get_network_fee()?;
        let account_fee = self.get_account_fee(id, additional_receipts)?;
        Some(AccountInfo {
            account,
            net_fee,
            account_fee,
        })
    }

    pub fn get_account_fee(&self, id: AccountId, additional_receipts: &[Receipt]) -> Option<Asset> {
        let mut count = 1;
        let mut delta = 0;

        macro_rules! handle_receipt_match {
            ($receipt:expr) => {
                let has_match = match &$receipt.tx {
                    TxVariant::V0(tx) => match tx {
                        TxVariantV0::OwnerTx(_) => false,
                        TxVariantV0::MintTx(_) => false,
                        TxVariantV0::CreateAccountTx(tx) => tx.creator == id,
                        TxVariantV0::UpdateAccountTx(tx) => tx.account_id == id,
                        TxVariantV0::TransferTx(tx) => tx.from == id,
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

        // Check positive amount
        macro_rules! check_pos_amt {
            ($asset:expr) => {
                if $asset.amount < 0 {
                    return Err(TxErr::InvalidAmount);
                }
            };
        }

        let tx = data.tx();

        if tx.sigs().len() > MAX_TX_SIGNATURES {
            return Err(TxErr::TooManySignatures);
        }

        match tx {
            TxVariant::V0(tx) => match tx {
                TxVariantV0::OwnerTx(new_owner) => {
                    check_zero_fee!(tx.fee);
                    if self
                        .get_account(new_owner.wallet, additional_receipts)
                        .is_none()
                    {
                        return Err(TxErr::AccountNotFound);
                    }

                    let prev_owner = match self.get_owner() {
                        TxVariant::V0(tx) => match tx {
                            TxVariantV0::OwnerTx(owner) => self
                                .get_account(owner.wallet, additional_receipts)
                                .expect("failed to get owner wallet account"),
                            _ => unreachable!(),
                        },
                    };

                    let data = EngineData {
                        script: prev_owner.script.into(),
                        tx_data: data.into(),
                        chain: self,
                        additional_receipts,
                    };
                    if let Err(e) = ScriptEngine::new(data).eval() {
                        return Err(TxErr::ScriptEval(e));
                    }
                    Ok(vec![])
                }
                TxVariantV0::MintTx(mint_tx) => {
                    check_zero_fee!(tx.fee);
                    check_pos_amt!(mint_tx.amount);

                    let owner = match self.get_owner() {
                        TxVariant::V0(tx) => match tx {
                            TxVariantV0::OwnerTx(owner) => self
                                .get_account(owner.wallet, additional_receipts)
                                .expect("failed to get owner wallet account"),
                            _ => unreachable!(),
                        },
                    };

                    let data = EngineData {
                        script: owner.script.into(),
                        tx_data: data.into(),
                        chain: self,
                        additional_receipts,
                    };
                    if let Err(e) = ScriptEngine::new(data).eval() {
                        return Err(TxErr::ScriptEval(e));
                    }

                    // Sanity check to ensure too many new coins can't be minted
                    self.indexer
                        .get_token_supply()
                        .checked_add(mint_tx.amount)
                        .ok_or(TxErr::Arithmetic)?;

                    Ok(vec![])
                }
                TxVariantV0::CreateAccountTx(create_account_tx) => {
                    let new_acc = &create_account_tx.account;

                    if new_acc.script.len() > MAX_SCRIPT_BYTE_SIZE {
                        return Err(TxErr::TxTooLarge);
                    } else if new_acc.destroyed {
                        return Err(TxErr::TxProhibited);
                    } else if !new_acc.permissions.is_valid() {
                        return Err(TxErr::InvalidAccountPermissions);
                    } else if self.indexer.account_exists(new_acc.id) {
                        return Err(TxErr::AccountAlreadyExists);
                    }

                    for receipt in additional_receipts {
                        match &receipt.tx {
                            TxVariant::V0(tx) => {
                                if let TxVariantV0::CreateAccountTx(tx) = tx {
                                    if tx.account.id == new_acc.id {
                                        return Err(TxErr::AccountAlreadyExists);
                                    }
                                }
                            }
                        }
                    }

                    let creator_acc_info = match self
                        .get_account_info(create_account_tx.creator, additional_receipts)
                    {
                        Some(info) => info,
                        None => return Err(TxErr::AccountNotFound),
                    };

                    {
                        let req_fee = creator_acc_info
                            .total_fee()
                            .ok_or(TxErr::Arithmetic)?
                            .checked_mul(GRAEL_ACC_CREATE_FEE_MULT)
                            .ok_or(TxErr::Arithmetic)?;
                        let min_bal = req_fee
                            .checked_mul(GRAEL_ACC_CREATE_MIN_BAL_MULT)
                            .ok_or(TxErr::Arithmetic)?;

                        if tx.fee < req_fee {
                            return Err(TxErr::InvalidFeeAmount);
                        } else if new_acc.balance < min_bal {
                            return Err(TxErr::InvalidAmount);
                        }
                    }

                    let bal = creator_acc_info
                        .account
                        .balance
                        .checked_sub(create_account_tx.fee)
                        .ok_or(TxErr::Arithmetic)?
                        .checked_sub(new_acc.balance)
                        .ok_or(TxErr::Arithmetic)?;
                    check_pos_amt!(bal);

                    let txid = data.txid();
                    if creator_acc_info
                        .account
                        .permissions
                        .verify(txid.as_ref(), &create_account_tx.signature_pairs)
                        .is_err()
                    {
                        return Err(TxErr::ScriptEval(EvalErr::new(
                            0,
                            EvalErrKind::ScriptRetFalse,
                        )));
                    }

                    Ok(vec![])
                }
                TxVariantV0::UpdateAccountTx(update_acc_tx) => {
                    let acc_info = match self
                        .get_account_info(update_acc_tx.account_id, additional_receipts)
                    {
                        Some(info) => info,
                        None => return Err(TxErr::AccountNotFound),
                    };

                    if acc_info.account.destroyed {
                        return Err(TxErr::TxProhibited);
                    } else if let Some(script) = &update_acc_tx.new_script {
                        if script.len() > MAX_SCRIPT_BYTE_SIZE {
                            return Err(TxErr::TxTooLarge);
                        }
                    } else if let Some(perms) = &update_acc_tx.new_permissions {
                        if !perms.is_valid() {
                            return Err(TxErr::InvalidAccountPermissions);
                        }
                    }

                    {
                        let req_fee = acc_info
                            .total_fee()
                            .ok_or(TxErr::Arithmetic)?
                            .checked_mul(GRAEL_ACC_CREATE_FEE_MULT)
                            .ok_or(TxErr::Arithmetic)?;

                        if tx.fee < req_fee {
                            return Err(TxErr::InvalidFeeAmount);
                        }
                    }

                    let bal = acc_info
                        .account
                        .balance
                        .checked_sub(update_acc_tx.fee)
                        .ok_or(TxErr::Arithmetic)?;
                    check_pos_amt!(bal);

                    let txid = data.txid();
                    if acc_info
                        .account
                        .permissions
                        .verify(txid.as_ref(), &update_acc_tx.signature_pairs)
                        .is_err()
                    {
                        return Err(TxErr::ScriptEval(EvalErr::new(
                            0,
                            EvalErrKind::ScriptRetFalse,
                        )));
                    }

                    Ok(vec![])
                }
                TxVariantV0::TransferTx(transfer) => {
                    if transfer.memo.len() > MAX_MEMO_BYTE_SIZE {
                        return Err(TxErr::TxTooLarge);
                    }
                    check_pos_amt!(transfer.amount);

                    let info = self
                        .get_account_info(transfer.from, additional_receipts)
                        .ok_or(TxErr::AccountNotFound)?;
                    if tx.fee < info.total_fee().ok_or(TxErr::Arithmetic)? {
                        return Err(TxErr::InvalidFeeAmount);
                    }

                    let bal = info
                        .account
                        .balance
                        .checked_sub(transfer.fee)
                        .ok_or(TxErr::Arithmetic)?
                        .checked_sub(transfer.amount)
                        .ok_or(TxErr::Arithmetic)?;
                    check_pos_amt!(bal);

                    let data = EngineData {
                        script: info.account.script.into(),
                        tx_data: data.into(),
                        chain: self,
                        additional_receipts,
                    };
                    let log = ScriptEngine::new(data).eval().map_err(TxErr::ScriptEval)?;
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
                batch.add_bal(tx.wallet, block.rewards());
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
                    batch.add_bal(tx.to, tx.amount);
                }
                TxVariantV0::CreateAccountTx(tx) => {
                    batch.sub_bal(tx.creator, tx.fee.checked_add(tx.account.balance).unwrap());
                    batch.insert_or_update_account(tx.account.clone());
                }
                TxVariantV0::UpdateAccountTx(tx) => {
                    let acc = batch.get_account_mut(tx.account_id);
                    acc.balance = acc.balance.checked_sub(tx.fee).unwrap();
                    if let Some(script) = &tx.new_script {
                        acc.script = script.clone();
                    }
                    if let Some(perms) = &tx.new_permissions {
                        acc.permissions = perms.clone();
                    }
                }
                TxVariantV0::TransferTx(tx) => {
                    batch.sub_bal(tx.from, tx.fee.checked_add(tx.amount).unwrap());
                    for entry in &receipt.log {
                        match entry {
                            LogEntry::Transfer(to_acc, amount) => batch.add_bal(*to_acc, *amount),
                            LogEntry::Destroy(to_acc) => {
                                let from_acc = batch.get_account_mut(tx.from);
                                let from_cur_bal = from_acc.balance;
                                from_acc.destroyed = true;
                                from_acc.balance = Asset::new(0);
                                batch.add_bal(*to_acc, from_cur_bal);
                            }
                        }
                    }
                }
            },
        }
    }

    pub fn create_genesis_block(&self, minter_key: KeyPair) -> GenesisBlockInfo {
        let info = GenesisBlockInfo::new(minter_key, 0);
        let timestamp = crate::get_epoch_time();

        let owner_wallet = Account {
            id: info.owner_id,
            balance: Asset::default(),
            script: info.script.clone(),
            permissions: Permissions {
                threshold: 2,
                keys: info.wallet_keys.iter().map(|kp| kp.0.clone()).collect(),
            },
            destroyed: false,
        };

        let create_account_tx = TxVariant::V0(TxVariantV0::CreateAccountTx(CreateAccountTx {
            base: Tx {
                nonce: 0,
                expiry: timestamp + 1,
                fee: Asset::default(),
                signature_pairs: Vec::new(),
            },
            account: owner_wallet.clone(),
            creator: 0,
        }));

        let owner_tx = TxVariant::V0(TxVariantV0::OwnerTx(OwnerTx {
            base: Tx {
                nonce: 0,
                expiry: timestamp + 1,
                fee: Asset::default(),
                signature_pairs: Vec::new(),
            },
            minter: info.minter_key.0.clone(),
            wallet: info.owner_id,
        }));

        let receipts = vec![
            Receipt {
                tx: create_account_tx,
                log: vec![],
            },
            Receipt {
                tx: owner_tx.clone(),
                log: vec![],
            },
        ];
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
        batch.insert_or_update_account(owner_wallet);
        batch.commit();
        self.indexer.set_index_status(IndexStatus::Complete);

        info
    }
}

pub struct GenesisBlockInfo {
    pub owner_id: AccountId,
    pub minter_key: KeyPair,
    pub wallet_keys: [KeyPair; 4],
    pub script: Script,
}

impl GenesisBlockInfo {
    pub fn new(minter_key: KeyPair, owner_id: AccountId) -> Self {
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
                    .push(OpFrame::AccountId(owner_id))
                    .push(OpFrame::OpCheckPerms),
            )
            .push(
                // Standard transfer function
                FnBuilder::new(1, OpFrame::OpDefine(vec![Arg::AccountId, Arg::Asset]))
                    .push(OpFrame::AccountId(owner_id))
                    .push(OpFrame::OpCheckPermsFastFail)
                    .push(OpFrame::OpTransfer)
                    .push(OpFrame::True),
            )
            .build()
            .unwrap();

        Self {
            owner_id,
            minter_key,
            wallet_keys,
            script,
        }
    }
}
