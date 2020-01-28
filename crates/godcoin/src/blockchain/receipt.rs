use crate::{asset::Asset, crypto::ScriptHash, serializer::*, tx::TxVariant};
use std::io::Cursor;

/// A receipt represents a transaction that has been executed.
#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
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
