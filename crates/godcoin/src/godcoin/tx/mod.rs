use sodiumoxide::crypto::sign::{PUBLICKEYBYTES, SIGNATUREBYTES};
use std::{
    borrow::Cow,
    io::Cursor,
    ops::{Deref, DerefMut},
};

use crate::{
    asset::Asset,
    crypto::{double_sha256, Digest, KeyPair, PublicKey, ScriptHash, SigPair},
    script::Script,
    serializer::*,
};

#[macro_use]
mod util;

pub mod tx_pool;

pub use self::tx_pool::*;

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TxType {
    OWNER = 0,
    MINT = 1,
    REWARD = 2,
    TRANSFER = 3,
}

pub trait SerializeTx {
    fn serialize(&self, v: &mut Vec<u8>);
}

pub trait DeserializeTx<T> {
    fn deserialize(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<T>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct TxId(Digest);

impl TxId {
    pub fn from_digest(txid: Digest) -> Self {
        TxId(txid)
    }
}

impl AsRef<[u8]> for TxId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TxPrecompData<'a> {
    tx: Cow<'a, TxVariant>,
    txid: TxId,
    bytes: Vec<u8>,
    sig_tx_suffix: usize,
}

impl<'a> TxPrecompData<'a> {
    pub fn from_tx<T>(tx: T) -> Self
    where
        T: Into<Cow<'a, TxVariant>>,
    {
        let tx = tx.into();
        let mut bytes = Vec::with_capacity(4096);
        tx.serialize(&mut bytes);
        let sigs_len = 1 + (tx.sigs().len() * (PUBLICKEYBYTES + SIGNATUREBYTES));
        let sig_tx_suffix = bytes.len() - sigs_len;

        let txid = TxId(double_sha256(&bytes));
        Self {
            tx,
            txid,
            bytes,
            sig_tx_suffix,
        }
    }

    #[inline]
    pub fn take(self) -> TxVariant {
        self.tx.into_owned()
    }

    #[inline]
    pub fn tx(&self) -> &TxVariant {
        &self.tx
    }

    #[inline]
    pub fn txid(&self) -> &TxId {
        &self.txid
    }

    #[inline]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[inline]
    pub fn bytes_without_sigs(&self) -> &[u8] {
        &self.bytes[..self.sig_tx_suffix]
    }
}

impl<'a> Into<Cow<'a, TxPrecompData<'a>>> for TxPrecompData<'a> {
    fn into(self) -> Cow<'a, TxPrecompData<'a>> {
        Cow::Owned(self)
    }
}

impl<'a> Into<Cow<'a, TxPrecompData<'a>>> for &'a TxPrecompData<'a> {
    fn into(self) -> Cow<'a, TxPrecompData<'a>> {
        Cow::Borrowed(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TxVariant {
    V0(TxVariantV0),
}

impl TxVariant {
    #[inline]
    pub fn precompute(self) -> TxPrecompData<'static> {
        TxPrecompData::from_tx(Cow::Owned(self))
    }

    #[inline]
    pub fn timestamp(&self) -> u64 {
        match self {
            TxVariant::V0(tx) => tx.timestamp,
        }
    }

    #[inline]
    pub fn sigs(&self) -> &[SigPair] {
        match self {
            TxVariant::V0(tx) => &tx.signature_pairs,
        }
    }

    #[inline]
    pub fn sigs_mut(&mut self) -> &mut Vec<SigPair> {
        match self {
            TxVariant::V0(tx) => &mut tx.signature_pairs,
        }
    }

    pub fn script(&self) -> Option<&Script> {
        match self {
            TxVariant::V0(var) => match var {
                TxVariantV0::OwnerTx(tx) => Some(&tx.script),
                TxVariantV0::MintTx(tx) => Some(&tx.script),
                TxVariantV0::RewardTx(_) => None,
                TxVariantV0::TransferTx(tx) => Some(&tx.script),
            },
        }
    }

    #[inline]
    pub fn sign(&self, key_pair: &KeyPair) -> SigPair {
        let mut buf = Vec::with_capacity(4096);
        self.serialize_without_sigs(&mut buf);
        key_pair.sign(&buf)
    }

    #[inline]
    pub fn append_sign(&mut self, key_pair: &KeyPair) {
        let pair = self.sign(key_pair);
        self.sigs_mut().push(pair);
    }

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        self.serialize_without_sigs(buf);
        match self {
            TxVariant::V0(var) => {
                macro_rules! serialize_sigs {
                    ($name:expr) => {{
                        buf.push($name.signature_pairs.len() as u8);
                        for sig in &$name.signature_pairs {
                            buf.push_sig_pair(sig)
                        }
                    }};
                }

                match var {
                    TxVariantV0::OwnerTx(tx) => serialize_sigs!(tx),
                    TxVariantV0::MintTx(tx) => serialize_sigs!(tx),
                    TxVariantV0::RewardTx(tx) => serialize_sigs!(tx),
                    TxVariantV0::TransferTx(tx) => serialize_sigs!(tx),
                }
            }
        };
    }

    pub fn serialize_without_sigs(&self, buf: &mut Vec<u8>) {
        match self {
            TxVariant::V0(var) => {
                // Tx version (2 bytes)
                buf.push_u16(0);

                match var {
                    TxVariantV0::OwnerTx(tx) => tx.serialize(buf),
                    TxVariantV0::MintTx(tx) => tx.serialize(buf),
                    TxVariantV0::RewardTx(tx) => tx.serialize(buf),
                    TxVariantV0::TransferTx(tx) => tx.serialize(buf),
                }
            }
        };
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> Option<TxVariant> {
        let tx_ver = cur.take_u16().ok()?;
        match tx_ver {
            0 => {
                let (base, tx_type) = Tx::deserialize_header(cur)?;
                let mut tx = match tx_type {
                    TxType::OWNER => TxVariantV0::OwnerTx(OwnerTx::deserialize(cur, base)?),
                    TxType::MINT => TxVariantV0::MintTx(MintTx::deserialize(cur, base)?),
                    TxType::REWARD => TxVariantV0::RewardTx(RewardTx::deserialize(cur, base)?),
                    TxType::TRANSFER => {
                        TxVariantV0::TransferTx(TransferTx::deserialize(cur, base)?)
                    }
                };
                tx.signature_pairs = {
                    let len = cur.take_u8().ok()?;
                    let mut sigs = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        sigs.push(cur.take_sig_pair().ok()?)
                    }
                    sigs
                };
                Some(TxVariant::V0(tx))
            }
            _ => None,
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

#[derive(Clone, Debug, PartialEq)]
pub enum TxVariantV0 {
    OwnerTx(OwnerTx),
    MintTx(MintTx),
    RewardTx(RewardTx),
    TransferTx(TransferTx),
}

impl Deref for TxVariantV0 {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        match self {
            TxVariantV0::OwnerTx(tx) => &tx.base,
            TxVariantV0::MintTx(tx) => &tx.base,
            TxVariantV0::RewardTx(tx) => &tx.base,
            TxVariantV0::TransferTx(tx) => &tx.base,
        }
    }
}

impl DerefMut for TxVariantV0 {
    fn deref_mut(&mut self) -> &mut Tx {
        match self {
            TxVariantV0::OwnerTx(tx) => &mut tx.base,
            TxVariantV0::MintTx(tx) => &mut tx.base,
            TxVariantV0::RewardTx(tx) => &mut tx.base,
            TxVariantV0::TransferTx(tx) => &mut tx.base,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tx {
    pub timestamp: u64,
    pub fee: Asset,
    pub signature_pairs: Vec<SigPair>,
}

impl Tx {
    fn serialize_header(&self, v: &mut Vec<u8>) {
        // The TxType is part of the header and needs to be pushed into the buffer first
        v.push_u64(self.timestamp);
        v.push_asset(self.fee);
    }

    fn deserialize_header(cur: &mut Cursor<&[u8]>) -> Option<(Tx, TxType)> {
        let tx_type = match cur.take_u8().ok()? {
            t if t == TxType::OWNER as u8 => TxType::OWNER,
            t if t == TxType::MINT as u8 => TxType::MINT,
            t if t == TxType::REWARD as u8 => TxType::REWARD,
            t if t == TxType::TRANSFER as u8 => TxType::TRANSFER,
            _ => return None,
        };
        let timestamp = cur.take_u64().ok()?;
        let fee = cur.take_asset().ok()?;
        let tx = Tx {
            timestamp,
            fee,
            signature_pairs: Vec::new(),
        };

        Some((tx, tx_type))
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
        v.push(TxType::OWNER as u8);
        self.serialize_header(v);
        v.push_pub_key(&self.minter);
        v.push_script_hash(&self.wallet);
        v.push_bytes(&self.script);
    }
}

impl DeserializeTx<OwnerTx> for OwnerTx {
    fn deserialize(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<OwnerTx> {
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
    pub attachment: Vec<u8>,
    pub attachment_name: String,
    pub script: Script,
}

impl SerializeTx for MintTx {
    fn serialize(&self, v: &mut Vec<u8>) {
        v.push(TxType::MINT as u8);
        self.serialize_header(v);
        v.push_script_hash(&self.to);
        v.push_asset(self.amount);
        v.push_bytes(&self.attachment);
        v.push_bytes(self.attachment_name.as_bytes());
        v.push_bytes(&self.script);
    }
}

impl DeserializeTx<MintTx> for MintTx {
    fn deserialize(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<Self> {
        let to = cur.take_script_hash().ok()?;
        let amount = cur.take_asset().ok()?;
        let attachment = cur.take_bytes().ok()?;
        let attachment_name = {
            let bytes = cur.take_bytes().ok()?;
            String::from_utf8(bytes).ok()?
        };
        let script = Script::from(cur.take_bytes().ok()?);
        Some(Self {
            base: tx,
            to,
            amount,
            attachment,
            attachment_name,
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
        v.push(TxType::REWARD as u8);
        self.serialize_header(v);
        v.push_script_hash(&self.to);
        v.push_asset(self.rewards);
    }
}

impl DeserializeTx<RewardTx> for RewardTx {
    fn deserialize(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<RewardTx> {
        let key = cur.take_script_hash().ok()?;
        let rewards = cur.take_asset().ok()?;

        Some(RewardTx {
            base: tx,
            to: key,
            rewards,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
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
        v.push(TxType::TRANSFER as u8);
        self.serialize_header(v);
        v.push_script_hash(&self.from);
        v.push_script_hash(&self.to);
        v.push_bytes(&self.script);
        v.push_asset(self.amount);
        v.push_bytes(&self.memo);
    }
}

impl DeserializeTx<TransferTx> for TransferTx {
    fn deserialize(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<TransferTx> {
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

tx_deref!(OwnerTx);
tx_deref!(MintTx);
tx_deref!(RewardTx);
tx_deref!(TransferTx);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto,
        script::{Builder, OpFrame},
    };

    macro_rules! cmp_base_tx {
        ($id:ident, $ts:expr, $fee:expr) => {
            assert_eq!($id.timestamp, $ts);
            assert_eq!($id.fee.to_string(), $fee);
        };
    }

    #[test]
    fn serialize_tx_with_sigs() {
        let to = crypto::KeyPair::gen();
        let reward_tx = TxVariant::V0(TxVariantV0::RewardTx(RewardTx {
            base: Tx {
                timestamp: 123,
                fee: get_asset("123.00000 GRAEL"),
                signature_pairs: vec![],
            },
            to: to.0.into(),
            rewards: get_asset("1.50000 GRAEL"),
        }));

        let mut v = vec![];
        reward_tx.serialize(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        TxVariant::deserialize(&mut c).unwrap();
    }

    #[test]
    fn serialize_owner() {
        let minter = crypto::KeyPair::gen();
        let wallet = crypto::KeyPair::gen();
        let owner_tx = OwnerTx {
            base: Tx {
                timestamp: 1230,
                fee: get_asset("123.00000 GRAEL"),
                signature_pairs: vec![],
            },
            minter: minter.0,
            wallet: wallet.0.clone().into(),
            script: wallet.0.into(),
        };

        let mut v = vec![];
        owner_tx.serialize(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let (base, tx_type) = Tx::deserialize_header(&mut c).unwrap();
        let dec = OwnerTx::deserialize(&mut c, base).unwrap();

        cmp_base_tx!(dec, 1230, "123.00000 GRAEL");
        assert_eq!(tx_type, TxType::OWNER);
        assert_eq!(owner_tx.minter, dec.minter);
        assert_eq!(owner_tx.wallet, dec.wallet);
    }

    #[test]
    fn serialize_mint() {
        let wallet = crypto::KeyPair::gen();
        let mint_tx = MintTx {
            base: Tx {
                timestamp: 1234,
                fee: get_asset("123.00000 GRAEL"),
                signature_pairs: vec![],
            },
            to: wallet.0.clone().into(),
            amount: get_asset("10.00000 GRAEL"),
            attachment: vec![1, 2, 3],
            attachment_name: "abc.pdf".to_owned(),
            script: wallet.0.into(),
        };

        let mut v = vec![];
        mint_tx.serialize(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let (base, tx_type) = Tx::deserialize_header(&mut c).unwrap();
        let dec = MintTx::deserialize(&mut c, base).unwrap();

        cmp_base_tx!(dec, 1234, "123.00000 GRAEL");
        assert_eq!(tx_type, TxType::MINT);
        assert_eq!(mint_tx.to, dec.to);
        assert_eq!(mint_tx.amount, dec.amount);
        assert_eq!(mint_tx, dec);
    }

    #[test]
    fn serialize_reward() {
        let to = crypto::KeyPair::gen();
        let reward_tx = RewardTx {
            base: Tx {
                timestamp: 123,
                fee: get_asset("123.00000 GRAEL"),
                signature_pairs: vec![],
            },
            to: to.0.into(),
            rewards: get_asset("1.50000 GRAEL"),
        };

        let mut v = vec![];
        reward_tx.serialize(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let (base, tx_type) = Tx::deserialize_header(&mut c).unwrap();
        let dec = RewardTx::deserialize(&mut c, base).unwrap();

        cmp_base_tx!(dec, 123, "123.00000 GRAEL");
        assert_eq!(tx_type, TxType::REWARD);
        assert_eq!(reward_tx.to, dec.to);
        assert_eq!(reward_tx.rewards, dec.rewards);
    }

    #[test]
    fn serialize_transfer() {
        let from = crypto::KeyPair::gen();
        let to = crypto::KeyPair::gen();
        let transfer_tx = TransferTx {
            base: Tx {
                timestamp: 1234567890,
                fee: get_asset("1.23000 GRAEL"),
                signature_pairs: vec![],
            },
            from: from.0.into(),
            to: to.0.into(),
            script: vec![1, 2, 3, 4].into(),
            amount: get_asset("1.00456 GRAEL"),
            memo: Vec::from(String::from("Hello world!").as_bytes()),
        };

        let mut v = vec![];
        transfer_tx.serialize(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let (base, tx_type) = Tx::deserialize_header(&mut c).unwrap();
        let dec = TransferTx::deserialize(&mut c, base).unwrap();

        cmp_base_tx!(dec, 1234567890, "1.23000 GRAEL");
        assert_eq!(tx_type, TxType::TRANSFER);
        assert_eq!(transfer_tx.from, dec.from);
        assert_eq!(transfer_tx.to, dec.to);
        assert_eq!(transfer_tx.script, vec![1, 2, 3, 4].into());
        assert_eq!(transfer_tx.amount.to_string(), dec.amount.to_string());
        assert_eq!(transfer_tx.memo, dec.memo);
    }

    #[test]
    fn tx_eq() {
        let tx_a = Tx {
            timestamp: 1000,
            fee: get_asset("10.00000 GRAEL"),
            signature_pairs: vec![KeyPair::gen().sign(b"hello world")],
        };
        let tx_b = tx_a.clone();
        assert_eq!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.timestamp = tx_b.timestamp + 1;
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.fee = get_asset("10.00000 GRAEL");
        assert_eq!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.fee = get_asset("100.00000 GRAEL");
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.fee = get_asset("1.00000 GRAEL");
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.signature_pairs
            .push(KeyPair::gen().sign(b"hello world"));
        assert_ne!(tx_a, tx_b);
    }

    #[test]
    fn transfer_tx_eq() {
        let tx_a = TransferTx {
            base: Tx {
                timestamp: 1000,
                fee: get_asset("10.00000 GRAEL"),
                signature_pairs: vec![KeyPair::gen().sign(b"hello world")],
            },
            from: KeyPair::gen().0.into(),
            to: KeyPair::gen().0.into(),
            script: Builder::new().push(OpFrame::True).build(),
            amount: get_asset("1.00000 GRAEL"),
            memo: vec![1, 2, 3],
        };

        let tx_b = tx_a.clone();
        assert_eq!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.base.fee = get_asset("10.00000 GRAEL");
        assert_eq!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.base.fee = get_asset("1.00000 GRAEL");
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
        tx_b.amount = get_asset("10.00000 GRAEL");
        assert_ne!(tx_a, tx_b);

        let mut tx_b = tx_a.clone();
        tx_b.memo = vec![1, 2, 3, 4];
        assert_ne!(tx_a, tx_b);
    }

    #[test]
    fn precomp_data_sig_split() {
        let tx = TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
            base: Tx {
                timestamp: 1000,
                fee: get_asset("10.00000 GRAEL"),
                signature_pairs: vec![KeyPair::gen().sign(b"hello world")],
            },
            from: KeyPair::gen().0.into(),
            to: KeyPair::gen().0.into(),
            script: Builder::new().push(OpFrame::True).build(),
            amount: get_asset("1.00000 GRAEL"),
            memo: vec![1, 2, 3],
        }));

        let mut buf = Vec::with_capacity(4096);
        tx.serialize_without_sigs(&mut buf);
        assert_eq!(tx.precompute().bytes_without_sigs(), buf.as_slice());
    }

    fn get_asset(s: &str) -> Asset {
        s.parse().unwrap()
    }
}
