use std::borrow::Cow;
use std::io::Cursor;

use crate::asset::Asset;
use crate::crypto::{KeyPair, PublicKey, ScriptHash, SigPair};
use crate::script::Script;
use crate::serializer::*;

#[macro_use]
mod util;

pub mod tx_type;
pub use self::tx_type::*;

pub trait EncodeTx {
    fn encode(&self, v: &mut Vec<u8>);
}

pub trait DecodeTx<T> {
    fn decode(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<T>;
}

pub trait SignTx {
    fn sign(&self, key_pair: &KeyPair) -> SigPair;
    fn append_sign(&mut self, key_pair: &KeyPair);
    fn verify_all(&self) -> bool;
    fn verify_keys(&self, keys: &[PublicKey]) -> bool;
}

#[derive(Debug, Clone)]
pub enum TxVariant {
    RewardTx(RewardTx),
    OwnerTx(OwnerTx),
    TransferTx(TransferTx),
}

impl TxVariant {
    pub fn encode(&self, v: &mut Vec<u8>) {
        match self {
            TxVariant::RewardTx(tx) => tx.encode(v),
            TxVariant::OwnerTx(tx) => tx.encode(v),
            TxVariant::TransferTx(tx) => tx.encode(v),
        };
    }

    pub fn encode_with_sigs(&self, v: &mut Vec<u8>) {
        macro_rules! encode_sigs {
            ($name:expr, $vec:expr) => {{
                $vec.push_u16($name.signature_pairs.len() as u16);
                for sig in &$name.signature_pairs {
                    $vec.push_sig_pair(sig)
                }
                $name.encode($vec);
            }};
        }

        match self {
            TxVariant::RewardTx(tx) => encode_sigs!(tx, v),
            TxVariant::OwnerTx(tx) => encode_sigs!(tx, v),
            TxVariant::TransferTx(tx) => encode_sigs!(tx, v),
        };
    }

    pub fn decode_with_sigs(cur: &mut Cursor<&[u8]>) -> Option<TxVariant> {
        let sigs = {
            let len = cur.take_u16().ok()?;
            let mut vec = Vec::with_capacity(len as usize);
            for _ in 0..len {
                vec.push(cur.take_sig_pair().ok()?)
            }
            vec
        };
        let mut base = Tx::decode_base(cur)?;
        base.signature_pairs = sigs;
        match base.tx_type {
            TxType::OWNER => Some(TxVariant::OwnerTx(OwnerTx::decode(cur, base)?)),
            TxType::REWARD => Some(TxVariant::RewardTx(RewardTx::decode(cur, base)?)),
            TxType::TRANSFER => Some(TxVariant::TransferTx(TransferTx::decode(cur, base)?)),
        }
    }
}

impl std::ops::Deref for TxVariant {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        match self {
            TxVariant::OwnerTx(tx) => &tx.base,
            TxVariant::RewardTx(tx) => &tx.base,
            TxVariant::TransferTx(tx) => &tx.base,
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

#[derive(Debug, Clone)]
pub struct Tx {
    pub tx_type: TxType,
    pub timestamp: u32,
    pub fee: Asset,
    pub signature_pairs: Vec<SigPair>,
}

impl Tx {
    fn encode_base(&self, v: &mut Vec<u8>) {
        v.push(self.tx_type as u8);
        v.push_u32(self.timestamp);
        v.push_asset(&self.fee);
    }

    fn decode_base(cur: &mut Cursor<&[u8]>) -> Option<Tx> {
        let tx_type = match cur.take_u8().ok()? {
            t if t == TxType::OWNER as u8 => TxType::OWNER,
            t if t == TxType::REWARD as u8 => TxType::REWARD,
            t if t == TxType::TRANSFER as u8 => TxType::TRANSFER,
            _ => return None,
        };
        let timestamp = cur.take_u32().ok()?;
        let fee = cur.take_asset().ok()?;

        Some(Tx {
            tx_type,
            timestamp,
            fee,
            signature_pairs: Vec::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct RewardTx {
    pub base: Tx,
    pub to: ScriptHash,
    pub rewards: Vec<Asset>,
}

impl EncodeTx for RewardTx {
    fn encode(&self, v: &mut Vec<u8>) {
        debug_assert_eq!(self.base.signature_pairs.len(), 0);
        self.encode_base(v);
        v.push_script_hash(&self.to);
        v.push_u32(self.rewards.len() as u32);
        for r in &self.rewards {
            v.push_asset(r)
        }
    }
}

impl DecodeTx<RewardTx> for RewardTx {
    fn decode(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<RewardTx> {
        assert_eq!(tx.tx_type, TxType::REWARD);
        let key = cur.take_script_hash().ok()?;

        let len = cur.take_u32().ok()?;
        let mut rewards = Vec::with_capacity(len as usize);
        for _ in 0..len {
            rewards.push(cur.take_asset().ok()?);
        }

        Some(RewardTx {
            base: tx,
            to: key,
            rewards,
        })
    }
}

#[derive(Debug, Clone)]
pub struct OwnerTx {
    pub base: Tx,
    pub minter: PublicKey, // Key that signs blocks
    pub wallet: PublicKey, // Hot wallet that receives rewards
}

impl EncodeTx for OwnerTx {
    fn encode(&self, v: &mut Vec<u8>) {
        self.encode_base(v);
        v.push_pub_key(&self.minter);
        v.push_pub_key(&self.wallet);
    }
}

impl DecodeTx<OwnerTx> for OwnerTx {
    fn decode(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<OwnerTx> {
        assert_eq!(tx.tx_type, TxType::OWNER);
        let minter = cur.take_pub_key().ok()?;
        let wallet = cur.take_pub_key().ok()?;
        Some(OwnerTx {
            base: tx,
            minter,
            wallet,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TransferTx {
    pub base: Tx,
    pub from: ScriptHash,
    pub to: ScriptHash,
    pub script: Script,
    pub amount: Asset,
    pub memo: Vec<u8>,
}

impl EncodeTx for TransferTx {
    fn encode(&self, v: &mut Vec<u8>) {
        self.encode_base(v);
        v.push_script_hash(&self.from);
        v.push_script_hash(&self.to);
        v.push_bytes(&self.script);
        v.push_asset(&self.amount);
        v.push_bytes(&self.memo);
    }
}

impl DecodeTx<TransferTx> for TransferTx {
    fn decode(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<TransferTx> {
        assert_eq!(tx.tx_type, TxType::TRANSFER);
        let from = cur.take_script_hash().ok()?;
        let to = cur.take_script_hash().ok()?;
        let script = Script::new(cur.take_bytes().ok()?);
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

tx_deref!(RewardTx);
tx_deref!(OwnerTx);
tx_deref!(TransferTx);

tx_sign!(OwnerTx);
tx_sign!(TransferTx);

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::crypto;

    macro_rules! cmp_base_tx {
        ($id:ident, $ty:expr, $ts:expr, $fee:expr) => {
            assert_eq!($id.tx_type, $ty);
            assert_eq!($id.timestamp, $ts);
            assert_eq!($id.fee.to_string(), $fee);
        };
    }

    #[test]
    fn test_encode_tx_with_sigs() {
        let to = crypto::KeyPair::gen_keypair();
        let reward_tx = TxVariant::RewardTx(RewardTx {
            base: Tx {
                tx_type: TxType::REWARD,
                timestamp: 123,
                fee: get_asset("123 GOLD"),
                signature_pairs: vec![],
            },
            to: to.0.into(),
            rewards: vec![get_asset("1.50 GOLD"), get_asset("1.0 SILVER")],
        });

        let mut v = vec![];
        reward_tx.encode_with_sigs(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        TxVariant::decode_with_sigs(&mut c).unwrap();
    }

    #[test]
    fn test_encode_reward() {
        let to = crypto::KeyPair::gen_keypair();
        let reward_tx = RewardTx {
            base: Tx {
                tx_type: TxType::REWARD,
                timestamp: 123,
                fee: get_asset("123 GOLD"),
                signature_pairs: vec![],
            },
            to: to.0.into(),
            rewards: vec![get_asset("1.50 GOLD"), get_asset("1.0 SILVER")],
        };

        let mut v = vec![];
        reward_tx.encode(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let base = Tx::decode_base(&mut c).unwrap();
        let dec = RewardTx::decode(&mut c, base).unwrap();

        cmp_base_tx!(dec, TxType::REWARD, 123, "123 GOLD");
        assert_eq!(reward_tx.to, dec.to);
        assert_eq!(reward_tx.rewards.len(), dec.rewards.len());
        assert_eq!(reward_tx.rewards[0].to_string(), dec.rewards[0].to_string());
        assert_eq!(reward_tx.rewards[1].to_string(), dec.rewards[1].to_string());
    }

    #[test]
    fn test_encode_owner() {
        let minter = crypto::KeyPair::gen_keypair();
        let wallet = crypto::KeyPair::gen_keypair();
        let owner_tx = OwnerTx {
            base: Tx {
                tx_type: TxType::OWNER,
                timestamp: 1230,
                fee: get_asset("123 GOLD"),
                signature_pairs: vec![],
            },
            minter: minter.0,
            wallet: wallet.0.into(),
        };

        let mut v = vec![];
        owner_tx.encode(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let base = Tx::decode_base(&mut c).unwrap();
        let dec = OwnerTx::decode(&mut c, base).unwrap();

        cmp_base_tx!(dec, TxType::OWNER, 1230, "123 GOLD");
        assert_eq!(owner_tx.minter, dec.minter);
        assert_eq!(owner_tx.wallet, dec.wallet);
    }

    #[test]
    fn test_encode_transfer() {
        let from = crypto::KeyPair::gen_keypair();
        let to = crypto::KeyPair::gen_keypair();
        let transfer_tx = TransferTx {
            base: Tx {
                tx_type: TxType::TRANSFER,
                timestamp: 1234567890,
                fee: get_asset("1.23 GOLD"),
                signature_pairs: vec![],
            },
            from: from.0.into(),
            to: to.0.into(),
            script: Script::new(vec![1, 2, 3, 4]),
            amount: get_asset("1.0456 GOLD"),
            memo: Vec::from(String::from("Hello world!").as_bytes()),
        };

        let mut v = vec![];
        transfer_tx.encode(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let base = Tx::decode_base(&mut c).unwrap();
        let dec = TransferTx::decode(&mut c, base).unwrap();

        cmp_base_tx!(dec, TxType::TRANSFER, 1234567890, "1.23 GOLD");
        assert_eq!(transfer_tx.from, dec.from);
        assert_eq!(transfer_tx.to, dec.to);
        assert_eq!(transfer_tx.script, Script::new(vec![1, 2, 3, 4]));
        assert_eq!(transfer_tx.amount.to_string(), dec.amount.to_string());
        assert_eq!(transfer_tx.memo, dec.memo);
    }

    #[test]
    fn test_verify_sigs() {
        let mut transfer_tx = TransferTx {
            base: Tx {
                tx_type: TxType::TRANSFER,
                timestamp: 1234567890,
                fee: get_asset("1.23 GOLD"),
                signature_pairs: vec![],
            },
            from: KeyPair::gen_keypair().0.into(),
            to: KeyPair::gen_keypair().0.into(),
            script: Script::new(vec![1, 2, 3, 4]),
            amount: get_asset("1.0456 GOLD"),
            memo: Vec::from(String::from("Hello world!").as_bytes()),
        };
        let keys = (0..=4)
            .map(|_| {
                let key = KeyPair::gen_keypair();
                transfer_tx.append_sign(&key);
                key.0
            })
            .collect::<Vec<_>>();

        // Test valid sigs with valid ordering
        assert!(transfer_tx.verify_all());
        assert!(transfer_tx.verify_keys(&keys));
        assert!(transfer_tx.verify_keys(&[keys[0].clone(), keys[3].clone(), keys[4].clone()]));

        // Test valid sigs with invalid ordering
        assert!(!transfer_tx.verify_keys(&[keys[1].clone(), keys[0].clone()]));
        assert!(!transfer_tx.verify_keys(&[keys[0].clone(), keys[2].clone(), keys[1].clone()]));

        // Test invalid key
        let bad_key = KeyPair::gen_keypair().0;
        assert!(!transfer_tx.verify_keys(&[bad_key.clone()]));
        assert!(!transfer_tx.verify_keys(&[keys[0].clone(), bad_key.clone()]));
        assert!(!transfer_tx.verify_keys(&[bad_key, keys[0].clone()]));
    }

    fn get_asset(s: &str) -> Asset {
        Asset::from_str(s).unwrap()
    }
}
