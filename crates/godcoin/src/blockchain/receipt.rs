use super::{index::TxManager, skip_flags, AddressInfo, Blockchain, TxErr};
use crate::{
    asset::Asset,
    constants::TX_MAX_EXPIRY_TIME,
    crypto::ScriptHash,
    serializer::*,
    tx::{TxPrecompData, TxVariant},
};
use std::{io::Cursor, mem, sync::Arc};

const DEFAULT_RECEIPT_CAPACITY: usize = 1024;

pub struct ReceiptPool {
    chain: Arc<Blockchain>,
    manager: TxManager,
    receipts: Vec<Receipt>,
}

impl ReceiptPool {
    pub fn new(chain: Arc<Blockchain>) -> Self {
        let manager = TxManager::new(chain.indexer());
        Self {
            chain,
            manager,
            receipts: Vec::with_capacity(DEFAULT_RECEIPT_CAPACITY),
        }
    }

    #[inline]
    pub fn get_address_info(&self, addr: &ScriptHash) -> Option<AddressInfo> {
        self.chain.get_address_info(addr, &self.receipts)
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
        } else if self.manager.has(data.txid()) {
            return Err(TxErr::TxDupe);
        }

        let log = self.chain.execute_tx(&data, &self.receipts, skip_flags)?;

        self.manager.insert(data.txid(), expiry);
        self.receipts.push(Receipt {
            tx: data.take(),
            log,
        });
        Ok(())
    }

    pub fn flush(&mut self) -> Vec<Receipt> {
        let mut receipts = Vec::with_capacity(DEFAULT_RECEIPT_CAPACITY);
        mem::swap(&mut receipts, &mut self.receipts);
        self.manager.purge_expired();
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
    Transfer(ScriptHash, Asset), // To address, amount
}

impl LogEntry {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Transfer(addr, amt) => {
                buf.push(0x00);
                buf.push_digest(&addr.0);
                buf.push_asset(*amt);
            }
        }
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> Option<Self> {
        let tag = cur.take_u8().ok()?;
        match tag {
            0x00 => {
                let addr = ScriptHash(cur.take_digest().ok()?);
                let amt = cur.take_asset().ok()?;
                Some(Self::Transfer(addr, amt))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{crypto::KeyPair, script::Script, tx::*};

    #[test]
    fn serialize_receipt() {
        let from_keys = KeyPair::gen();
        let to_keys = KeyPair::gen();
        let amount = "123.45678 TEST".parse().unwrap();
        let receipt = Receipt {
            tx: TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: Tx {
                    nonce: 111,
                    expiry: 1234567890,
                    fee: Asset::default(),
                    signature_pairs: Vec::new(),
                },
                from: from_keys.0.into(),
                to: to_keys.0.clone().into(),
                script: Script::new(vec![21, 22, 23, 24]),
                call_fn: 0,
                amount,
                memo: vec![1, 2, 3, 4],
            })),
            log: vec![LogEntry::Transfer(to_keys.0.into(), amount)],
        };

        let mut buf = Vec::with_capacity(4096);
        receipt.serialize(&mut buf);
        let deserialized_receipt = Receipt::deserialize(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(receipt, deserialized_receipt);
    }
}
