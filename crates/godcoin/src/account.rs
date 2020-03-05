use crate::{asset::Asset, crypto::PublicKey, script::Script, serializer::*};
use std::io::{self, Cursor};

pub type AccountId = u64;

pub const MAX_PERM_KEYS: u8 = 8;
pub const IMMUTABLE_ACCOUNT_THRESHOLD: u8 = 0xFF;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    pub balance: Asset,
    pub script: Script,
    pub permissions: Permissions,
    pub destroyed: bool,
}

impl Account {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.push_u64(self.id);
        buf.push_asset(self.balance);
        buf.push_bytes(&self.script);
        self.permissions.serialize(buf);
        buf.push(self.destroyed as u8);
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let id = cur.take_u64()?;
        let balance = cur.take_asset()?;
        let script = Script::new(cur.take_bytes()?);
        let permissions = Permissions::deserialize(cur)?;
        let destroyed = cur.take_u8()? != 0;
        Ok(Self {
            id,
            balance,
            script,
            permissions,
            destroyed,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permissions {
    pub threshold: u8,
    pub keys: Vec<PublicKey>,
}

impl Permissions {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.push(self.threshold);
        buf.push(self.keys.len() as u8);
        for key in &self.keys {
            buf.push_pub_key(key);
        }
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let threshold = cur.take_u8()?;
        let key_len = cur.take_u8()?;
        let mut keys = Vec::with_capacity(usize::from(key_len));
        for _ in 0..key_len {
            keys.push(cur.take_pub_key()?);
        }
        Ok(Self { threshold, keys })
    }
}
