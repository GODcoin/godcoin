use crate::{
    blockchain::index::TxManager,
    constants::TX_MAX_EXPIRY_TIME,
    prelude::{verify::*, AddressInfo, Blockchain, ScriptHash, TxPrecompData, TxVariant},
};
use std::{mem, sync::Arc};

const DEFAULT_TX_CAP: usize = 1024;

pub struct TxPool {
    chain: Arc<Blockchain>,
    manager: TxManager,
    txs: Vec<TxVariant>,
}

impl TxPool {
    pub fn new(chain: Arc<Blockchain>) -> Self {
        let manager = TxManager::new(chain.indexer());
        Self {
            chain,
            manager,
            txs: Vec::with_capacity(DEFAULT_TX_CAP),
        }
    }

    #[inline]
    pub fn get_address_info(&self, addr: &ScriptHash) -> Option<AddressInfo> {
        self.chain.get_address_info(addr, &self.txs)
    }

    pub fn push(&mut self, data: TxPrecompData, skip_flags: SkipFlags) -> Result<(), TxErr> {
        let current_time = crate::get_epoch_time();

        let expiry = data.tx().expiry();
        if expiry <= current_time || expiry - current_time > TX_MAX_EXPIRY_TIME {
            return Err(TxErr::TxExpired);
        } else if self.manager.has(data.txid()) {
            return Err(TxErr::TxDupe);
        }
        self.chain.verify_tx(&data, &self.txs, skip_flags)?;

        self.manager.insert(data.txid(), expiry);
        self.txs.push(data.take());
        Ok(())
    }

    pub fn flush(&mut self) -> Vec<TxVariant> {
        let mut transactions = Vec::with_capacity(DEFAULT_TX_CAP);
        mem::swap(&mut transactions, &mut self.txs);
        self.manager.purge_expired();
        transactions
    }
}
