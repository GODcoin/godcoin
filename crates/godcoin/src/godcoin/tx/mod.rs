use std::{
    borrow::Cow,
    io::Cursor,
    ops::{Deref, DerefMut},
};

use crate::{
    asset::Asset,
    crypto::{KeyPair, PublicKey, ScriptHash, SigPair},
    script::Script,
    serializer::*,
};

#[macro_use]
mod util;

pub mod tx_type;
pub use self::tx_type::*;

pub trait SerializeTx {
    fn serialize(&self, v: &mut Vec<u8>);
}

pub trait DeserializeTx<T> {
    fn deserialize(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<T>;
}

pub trait SignTx {
    fn sign(&self, key_pair: &KeyPair) -> SigPair;
    fn append_sign(&mut self, key_pair: &KeyPair);
}

#[derive(Clone, Debug, PartialEq)]
pub enum TxVariant {
    OwnerTx(OwnerTx),
    MintTx(MintTx),
    RewardTx(RewardTx),
    TransferTx(TransferTx),
}

impl TxVariant {
    pub fn serialize_without_sigs(&self, buf: &mut Vec<u8>) {
        match self {
            TxVariant::OwnerTx(tx) => tx.serialize(buf),
            TxVariant::MintTx(tx) => tx.serialize(buf),
            TxVariant::RewardTx(tx) => tx.serialize(buf),
            TxVariant::TransferTx(tx) => tx.serialize(buf),
        };
    }

    pub fn serialize(&self, v: &mut Vec<u8>) {
        macro_rules! serialize_sigs {
            ($name:expr, $vec:expr) => {{
                $vec.push_u16($name.signature_pairs.len() as u16);
                for sig in &$name.signature_pairs {
                    $vec.push_sig_pair(sig)
                }
                $name.serialize($vec);
            }};
        }

        match self {
            TxVariant::OwnerTx(tx) => serialize_sigs!(tx, v),
            TxVariant::MintTx(tx) => serialize_sigs!(tx, v),
            TxVariant::RewardTx(tx) => serialize_sigs!(tx, v),
            TxVariant::TransferTx(tx) => serialize_sigs!(tx, v),
        };
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> Option<TxVariant> {
        let sigs = {
            let len = cur.take_u16().ok()?;
            let mut vec = Vec::with_capacity(len as usize);
            for _ in 0..len {
                vec.push(cur.take_sig_pair().ok()?)
            }
            vec
        };
        let mut base = Tx::deserialize_header(cur)?;
        base.signature_pairs = sigs;
        match base.tx_type {
            TxType::OWNER => Some(TxVariant::OwnerTx(OwnerTx::deserialize(cur, base)?)),
            TxType::MINT => Some(TxVariant::MintTx(MintTx::deserialize(cur, base)?)),
            TxType::REWARD => Some(TxVariant::RewardTx(RewardTx::deserialize(cur, base)?)),
            TxType::TRANSFER => Some(TxVariant::TransferTx(TransferTx::deserialize(cur, base)?)),
        }
    }
}

impl Deref for TxVariant {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        match self {
            TxVariant::OwnerTx(tx) => &tx.base,
            TxVariant::MintTx(tx) => &tx.base,
            TxVariant::RewardTx(tx) => &tx.base,
            TxVariant::TransferTx(tx) => &tx.base,
        }
    }
}

impl DerefMut for TxVariant {
    fn deref_mut(&mut self) -> &mut Tx {
        match self {
            TxVariant::OwnerTx(tx) => &mut tx.base,
            TxVariant::MintTx(tx) => &mut tx.base,
            TxVariant::RewardTx(tx) => &mut tx.base,
            TxVariant::TransferTx(tx) => &mut tx.base,
        }
    }
}

impl<'a> Into<Cow<'a, TxVariant>> for TxVariant {
    fn into(self) -> Cow<'a, TxVariant> {
        Cow::Owned(self)
    }
}

impl<'a> Into<Cow<'a, TxVariant>> for &'a TxVariant {
    fn into(self) -> Cow<'a, TxVariant> {
        Cow::Borrowed(self)
    }
}

#[derive(Clone, Debug)]
pub struct Tx {
    pub tx_type: TxType,
    pub timestamp: u64,
    pub fee: Asset,
    pub signature_pairs: Vec<SigPair>,
}

impl Tx {
    fn serialize_header(&self, v: &mut Vec<u8>) {
        v.push(self.tx_type as u8);
        v.push_u64(self.timestamp);
        v.push_asset(&self.fee);
    }

    fn deserialize_header(cur: &mut Cursor<&[u8]>) -> Option<Tx> {
        let tx_type = match cur.take_u8().ok()? {
            t if t == TxType::OWNER as u8 => TxType::OWNER,
            t if t == TxType::MINT as u8 => TxType::MINT,
            t if t == TxType::REWARD as u8 => TxType::REWARD,
            t if t == TxType::TRANSFER as u8 => TxType::TRANSFER,
            _ => return None,
        };
        let timestamp = cur.take_u64().ok()?;
        let fee = cur.take_asset().ok()?;

        Some(Tx {
            tx_type,
            timestamp,
            fee,
            signature_pairs: Vec::new(),
        })
    }
}

impl PartialEq for Tx {
    fn eq(&self, other: &Self) -> bool {
        self.tx_type == other.tx_type
            && self.timestamp == other.timestamp
            && self.fee.eq(&other.fee).unwrap_or(false)
            && self.signature_pairs == other.signature_pairs
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnerTx {
    pub base: Tx,
    pub minter: PublicKey,  // Key that signs blocks
    pub wallet: ScriptHash, // Hot wallet that receives rewards
    pub script: Script,     // Hot wallet previous script
}

impl SerializeTx for OwnerTx {
    fn serialize(&self, v: &mut Vec<u8>) {
        self.serialize_header(v);
        v.push_pub_key(&self.minter);
        v.push_script_hash(&self.wallet);
        v.push_bytes(&self.script);
    }
}

impl DeserializeTx<OwnerTx> for OwnerTx {
    fn deserialize(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<OwnerTx> {
        assert_eq!(tx.tx_type, TxType::OWNER);
        let minter = cur.take_pub_key().ok()?;
        let wallet = cur.take_script_hash().ok()?;
        let script = cur.take_bytes().ok()?.into();
        Some(OwnerTx {
            base: tx,
            minter,
            wallet,
            script,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MintTx {
    pub base: Tx,
    pub to: ScriptHash,
    pub amount: Asset,
    pub script: Script,
}

impl SerializeTx for MintTx {
    fn serialize(&self, v: &mut Vec<u8>) {
        self.serialize_header(v);
        v.push_script_hash(&self.to);
        v.push_asset(&self.amount);
        v.push_bytes(&self.script);
    }
}

impl DeserializeTx<MintTx> for MintTx {
    fn deserialize(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<Self> {
        assert_eq!(tx.tx_type, TxType::MINT);
        let to = cur.take_script_hash().ok()?;
        let amount = cur.take_asset().ok()?;
        let script = Script::from(cur.take_bytes().ok()?);
        Some(Self {
            base: tx,
            to,
            amount,
            script,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RewardTx {
    pub base: Tx,
    pub to: ScriptHash,
    pub rewards: Asset,
}

impl SerializeTx for RewardTx {
    fn serialize(&self, v: &mut Vec<u8>) {
        debug_assert_eq!(self.base.signature_pairs.len(), 0);
        self.serialize_header(v);
        v.push_script_hash(&self.to);
        v.push_asset(&self.rewards);
    }
}

impl DeserializeTx<RewardTx> for RewardTx {
    fn deserialize(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<RewardTx> {
        assert_eq!(tx.tx_type, TxType::REWARD);
        let key = cur.take_script_hash().ok()?;
        let rewards = cur.take_asset().ok()?;

        Some(RewardTx {
            base: tx,
            to: key,
            rewards,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TransferTx {
    pub base: Tx,
    pub from: ScriptHash,
    pub to: ScriptHash,
    pub script: Script,
    pub amount: Asset,
    pub memo: Vec<u8>,
}

impl SerializeTx for TransferTx {
    fn serialize(&self, v: &mut Vec<u8>) {
        self.serialize_header(v);
        v.push_script_hash(&self.from);
        v.push_script_hash(&self.to);
        v.push_bytes(&self.script);
        v.push_asset(&self.amount);
        v.push_bytes(&self.memo);
    }
}

impl DeserializeTx<TransferTx> for TransferTx {
    fn deserialize(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<TransferTx> {
        assert_eq!(tx.tx_type, TxType::TRANSFER);
        let from = cur.take_script_hash().ok()?;
        let to = cur.take_script_hash().ok()?;
        let script = cur.take_bytes().ok()?.into();
        let amount = cur.take_asset().ok()?;
        let memo = cur.take_bytes().ok()?;
        Some(TransferTx {
            base: tx,
            from,
            to,
            script,
            amount,
            memo,
        })
    }
}

impl PartialEq for TransferTx {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.from == other.from
            && self.to == other.to
            && self.script == other.script
            && self.amount.eq(&other.amount).unwrap_or(false)
            && self.memo == other.memo
    }
}

tx_deref!(OwnerTx);
tx_deref!(MintTx);
tx_deref!(RewardTx);
tx_deref!(TransferTx);

tx_sign!(OwnerTx);
tx_sign!(MintTx);
tx_sign!(TransferTx);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto,
        script::{Builder, OpFrame},
    };

    macro_rules! cmp_base_tx {
        ($id:ident, $ty:expr, $ts:expr, $fee:expr) => {
            assert_eq!($id.tx_type, $ty);
            assert_eq!($id.timestamp, $ts);
            assert_eq!($id.fee.to_string(), $fee);
        };
    }

    #[test]
    fn test_serialize_tx_with_sigs() {
        let to = crypto::KeyPair::gen();
        let reward_tx = TxVariant::RewardTx(RewardTx {
            base: Tx {
                tx_type: TxType::REWARD,
                timestamp: 123,
                fee: get_asset("123 GRAEL"),
                signature_pairs: vec![],
            },
            to: to.0.into(),
            rewards: get_asset("1.50 GRAEL"),
        });

        let mut v = vec![];
        reward_tx.serialize(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        TxVariant::deserialize(&mut c).unwrap();
    }

    #[test]
    fn test_serialize_owner() {
        let minter = crypto::KeyPair::gen();
        let wallet = crypto::KeyPair::gen();
        let owner_tx = OwnerTx {
            base: Tx {
                tx_type: TxType::OWNER,
                timestamp: 1230,
                fee: get_asset("123 GRAEL"),
                signature_pairs: vec![],
            },
            minter: minter.0,
            wallet: wallet.0.clone().into(),
            script: wallet.0.into(),
        };

        let mut v = vec![];
        owner_tx.serialize(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let base = Tx::deserialize_header(&mut c).unwrap();
        let dec = OwnerTx::deserialize(&mut c, base).unwrap();

        cmp_base_tx!(dec, TxType::OWNER, 1230, "123 GRAEL");
        assert_eq!(owner_tx.minter, dec.minter);
        assert_eq!(owner_tx.wallet, dec.wallet);
    }

    #[test]
    fn test_serialize_mint() {
        let wallet = crypto::KeyPair::gen();
        let mint_tx = MintTx {
            base: Tx {
                tx_type: TxType::MINT,
                timestamp: 1234,
                fee: get_asset("123 GRAEL"),
                signature_pairs: vec![],
            },
            to: wallet.0.clone().into(),
            amount: get_asset("10 GRAEL"),
            script: wallet.0.into(),
        };

        let mut v = vec![];
        mint_tx.serialize(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let base = Tx::deserialize_header(&mut c).unwrap();
        let dec = MintTx::deserialize(&mut c, base).unwrap();

        cmp_base_tx!(dec, TxType::MINT, 1234, "123 GRAEL");
        assert_eq!(mint_tx.to, dec.to);
        assert_eq!(mint_tx.amount, dec.amount);
    }

    #[test]
    fn test_serialize_reward() {
        let to = crypto::KeyPair::gen();
        let reward_tx = RewardTx {
            base: Tx {
                tx_type: TxType::REWARD,
                timestamp: 123,
                fee: get_asset("123 GRAEL"),
                signature_pairs: vec![],
            },
            to: to.0.into(),
            rewards: get_asset("1.50 GRAEL"),
        };

        let mut v = vec![];
        reward_tx.serialize(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let base = Tx::deserialize_header(&mut c).unwrap();
        let dec = RewardTx::deserialize(&mut c, base).unwrap();

        cmp_base_tx!(dec, TxType::REWARD, 123, "123 GRAEL");
        assert_eq!(reward_tx.to, dec.to);
        assert_eq!(reward_tx.rewards, dec.rewards);
    }

    #[test]
    fn test_serialize_transfer() {
        let from = crypto::KeyPair::gen();
        let to = crypto::KeyPair::gen();
        let transfer_tx = TransferTx {
            base: Tx {
                tx_type: TxType::TRANSFER,
                timestamp: 1234567890,
                fee: get_asset("1.23 GRAEL"),
                signature_pairs: vec![],
            },
            from: from.0.into(),
            to: to.0.into(),
            script: vec![1, 2, 3, 4].into(),
            amount: get_asset("1.0456 GRAEL"),
            memo: Vec::from(String::from("Hello world!").as_bytes()),
        };

        let mut v = vec![];
        transfer_tx.serialize(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let base = Tx::deserialize_header(&mut c).unwrap();
        let dec = TransferTx::deserialize(&mut c, base).unwrap();

        cmp_base_tx!(dec, TxType::TRANSFER, 1234567890, "1.23 GRAEL");
        assert_eq!(transfer_tx.from, dec.from);
        assert_eq!(transfer_tx.to, dec.to);
        assert_eq!(transfer_tx.script, vec![1, 2, 3, 4].into());
        assert_eq!(transfer_tx.amount.to_string(), dec.amount.to_string());
        assert_eq!(transfer_tx.memo, dec.memo);
    }

    #[test]
    fn test_tx_eq() {
        let tx_a = Tx {
            tx_type: TxType::MINT,
            timestamp: 1000,
            fee: get_asset("10 GRAEL"),
            signature_pairs: vec![KeyPair::gen().sign(b"hello world")],
        };
        let tx_b = tx_a.clone();
        assert_eq!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.tx_type = TxType::OWNER;
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.timestamp = tx_b.timestamp + 1;
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.fee = get_asset("10.0 GRAEL");
        assert_eq!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.fee = get_asset("100 GRAEL");
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.fee = get_asset("1.0 GRAEL");
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.signature_pairs
            .push(KeyPair::gen().sign(b"hello world"));
        assert_ne!(tx_a, tx_b);
    }

    #[test]
    fn test_transfer_tx_eq() {
        let tx_a = TransferTx {
            base: Tx {
                tx_type: TxType::TRANSFER,
                timestamp: 1000,
                fee: get_asset("10 GRAEL"),
                signature_pairs: vec![KeyPair::gen().sign(b"hello world")],
            },
            from: KeyPair::gen().0.into(),
            to: KeyPair::gen().0.into(),
            script: Builder::new().push(OpFrame::True).build(),
            amount: get_asset("1.0 GRAEL"),
            memo: vec![1, 2, 3],
        };

        let tx_b = tx_a.clone();
        assert_eq!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.base.fee = get_asset("1.0 GRAEL");
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.from = KeyPair::gen().0.into();
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.to = KeyPair::gen().0.into();
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.script = Builder::new().push(OpFrame::False).build();
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.amount = get_asset("10 GRAEL");
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.memo = vec![1, 2, 3, 4];
        assert_ne!(tx_a, tx_b);
    }

    fn get_asset(s: &str) -> Asset {
        s.parse().unwrap()
    }
}
