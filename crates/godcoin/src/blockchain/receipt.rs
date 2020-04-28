use super::{skip_flags, AccountInfo, Blockchain, Indexer, TxErr};
use crate::{
    account::AccountId,
    asset::Asset,
    constants::TX_MAX_EXPIRY_TIME,
    serializer::*,
    tx::{TxPrecompData, TxVariant},
};
use std::{io::Cursor, mem, sync::Arc};

const DEFAULT_RECEIPT_CAPACITY: usize = 1024;

pub struct ReceiptPool {
    chain: Arc<Blockchain>,
    indexer: Arc<Indexer>,
    receipts: Vec<Receipt>,
}

impl ReceiptPool {
    pub fn new(chain: Arc<Blockchain>) -> Self {
        let indexer = chain.indexer();
        Self {
            chain,
            indexer,
            receipts: Vec::with_capacity(DEFAULT_RECEIPT_CAPACITY),
        }
    }

    #[inline]
    pub fn get_account_info(&self, id: AccountId) -> Option<AccountInfo> {
        self.chain.get_account_info(id, &self.receipts)
    }

    pub fn push(
        &mut self,
        data: TxPrecompData,
        skip_flags: skip_flags::SkipFlags,
    ) -> Result<(), TxErr> {
        let current_time = crate::get_epoch_time();

        let expiry = data.tx().expiry();
        if expiry <= current_time || expiry - current_time > TX_MAX_EXPIRY_TIME {
            return Err(TxErr::TxExpired);
        } else if self.indexer.has_txid(data.txid()) {
            return Err(TxErr::TxDupe);
        }

        let log = self.chain.execute_tx(&data, &self.receipts, skip_flags)?;

        self.indexer.insert_txid(data.txid(), expiry);
        self.receipts.push(Receipt {
            tx: data.take(),
            log,
        });
        Ok(())
    }

    pub fn flush(&mut self) -> Vec<Receipt> {
        let mut receipts = Vec::with_capacity(DEFAULT_RECEIPT_CAPACITY);
        mem::swap(&mut receipts, &mut self.receipts);
        self.indexer.purge_expired_txids();
        receipts
    }
}

/// A receipt represents a transaction that has been executed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub tx: TxVariant,
    pub log: Vec<LogEntry>,
}

impl Receipt {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        self.tx.serialize(buf);
        buf.push_u16(self.log.len() as u16);
        for entry in &self.log {
            entry.serialize(buf);
        }
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> Option<Self> {
        let tx = TxVariant::deserialize(cur)?;
        let log_len = cur.take_u16().ok()?;
        let mut log = Vec::with_capacity(log_len as usize);
        for _ in 0..log_len {
            log.push(LogEntry::deserialize(cur)?);
        }
        Some(Receipt { tx, log })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogEntry {
    /// Sends tokens to the specified account
    Transfer(AccountId, Asset), // To account, amount
    /// Destroys an account and sends any remaining funds to the specified account
    Destroy(AccountId),
}

impl LogEntry {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Transfer(acc, amt) => {
                buf.push(0x00);
                buf.push_u64(*acc);
                buf.push_asset(*amt);
            }
            Self::Destroy(acc) => {
                buf.push(0x01);
                buf.push_u64(*acc);
            }
        }
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> Option<Self> {
        let tag = cur.take_u8().ok()?;
        match tag {
            0x00 => {
                let acc = cur.take_u64().ok()?;
                let amt = cur.take_asset().ok()?;
                Some(Self::Transfer(acc, amt))
            }
            0x01 => {
                let acc = cur.take_u64().ok()?;
                Some(Self::Destroy(acc))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::*;

    #[test]
    fn serialize_receipt() {
        let amount = "123.45678 TEST".parse().unwrap();
        let receipt = Receipt {
            tx: TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: Tx {
                    nonce: 111,
                    expiry: 1234567890,
                    fee: Asset::default(),
                    signature_pairs: Vec::new(),
                },
                from: 0xFFFF,
                call_fn: 0,
                args: vec![0x01, 0x02, 0x03, 0x04, 0x05],
                amount,
                memo: vec![1, 2, 3, 4],
            })),
            log: vec![LogEntry::Transfer(123456, amount)],
        };

        let mut buf = Vec::with_capacity(4096);
        receipt.serialize(&mut buf);
        let deserialized_receipt = Receipt::deserialize(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(receipt, deserialized_receipt);
    }
}
