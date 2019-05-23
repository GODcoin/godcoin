use crate::prelude::{util, verify, Blockchain, TxPrecompData, TxVariant};
use std::{mem, sync::Arc};

const DEFAULT_TX_CAP: usize = 1024;

pub struct TxPool {
    chain: Arc<Blockchain>,
    txs: Vec<TxVariant>,
}

impl TxPool {
    pub fn new(chain: Arc<Blockchain>) -> Self {
        Self {
            chain,
            txs: Vec::with_capacity(DEFAULT_TX_CAP),
        }
    }

    pub fn push(
        &mut self,
        data: TxPrecompData,
        config: verify::Config,
    ) -> Result<(), verify::TxErr> {
        let current_time = util::get_epoch_ms();

        let tx = data.tx();
        if (tx.timestamp < current_time - crate::constants::TX_EXPIRY_TIME)
            || (tx.timestamp > current_time + 3000)
        {
            return Err(verify::TxErr::TxExpired);
        }

        self.chain.verify_tx(&data, &self.txs, config)?;

        // TODO: push into the indexer and check for dupes
        self.txs.push(data.take());
        Ok(())
    }

    pub fn flush(&mut self) -> Vec<TxVariant> {
        let mut transactions = Vec::with_capacity(DEFAULT_TX_CAP);
        mem::swap(&mut transactions, &mut self.txs);
        transactions
    }
}
