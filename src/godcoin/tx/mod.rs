use ::std::io::Cursor;

use crypto::{PublicKey, KeyPair, SigPair};
use serializer::*;
use asset::Asset;

#[macro_use] mod util;

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
}

#[derive(Debug, Clone)]
pub enum TxVariant {
    RewardTx(RewardTx),
    BondTx(BondTx),
    TransferTx(TransferTx)
}

impl TxVariant {
    pub fn encode_with_sigs(&self, v: &mut Vec<u8>) {
        macro_rules! encode_sigs {
            ($name:expr, $vec:expr) => {
                {
                    $vec.push_u16($name.signature_pairs.len() as u16);
                    for sig in &$name.signature_pairs { $vec.push_sig_pair(sig) };
                    $name.encode($vec);
                }
            }
        }

        match self {
            TxVariant::RewardTx(tx) => { encode_sigs!(tx, v) },
            TxVariant::BondTx(tx) => { encode_sigs!(tx, v) },
            TxVariant::TransferTx(tx) => { encode_sigs!(tx, v) }
        };
    }

    pub fn decode_with_sigs(cur: &mut Cursor<&[u8]>) -> Option<TxVariant> {
        let sigs = {
            let len = cur.take_u16()?;
            let mut vec = Vec::with_capacity(len as usize);
            for _ in 0..len { vec.push(cur.take_sig_pair()?) };
            vec
        };
        let mut base = Tx::decode_base(cur)?;
        base.signature_pairs = sigs;
        match base.tx_type {
            TxType::REWARD => Some(TxVariant::RewardTx(RewardTx::decode(cur, base)?)),
            TxType::BOND => Some(TxVariant::BondTx(BondTx::decode(cur, base)?)),
            TxType::TRANSFER => Some(TxVariant::TransferTx(TransferTx::decode(cur, base)?))
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tx {
    pub tx_type: TxType,
    pub timestamp: u32,
    pub fee: Asset,
    pub signature_pairs: Vec<SigPair>
}

impl Tx {
    fn encode_base(&self, v: &mut Vec<u8>) {
        v.push(self.tx_type as u8);
        v.push_u32(self.timestamp);
        v.push_asset(&self.fee);
    }

    fn decode_base(cur: &mut Cursor<&[u8]>) -> Option<Tx> {
        let tx_type = match cur.take_u8()? {
            t if t == TxType::REWARD as u8 => TxType::REWARD,
            t if t == TxType::BOND as u8 => TxType::BOND,
            t if t == TxType::TRANSFER as u8 => TxType::TRANSFER,
            _ => return None
        };
        let timestamp = cur.take_u32()?;
        let fee = cur.take_asset()?;

        Some(Tx {
            tx_type,
            timestamp,
            fee,
            signature_pairs: Vec::new()
        })
    }
}

#[derive(Debug, Clone)]
pub struct RewardTx {
    pub base: Tx,
    pub to: PublicKey,
    pub rewards: Vec<Asset>
}

impl EncodeTx for RewardTx {
    fn encode(&self, v: &mut Vec<u8>) {
        debug_assert_eq!(self.base.signature_pairs.len(), 0);
        self.encode_base(v);
        v.push_pub_key(&self.to);
        v.push_u32(self.rewards.len() as u32);
        for r in &self.rewards { v.push_asset(r) };
    }
}

impl DecodeTx<RewardTx> for RewardTx {
    fn decode(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<RewardTx> {
        assert_eq!(tx.tx_type, TxType::REWARD);
        let key = cur.take_pub_key()?;

        let len = cur.take_u32()?;
        let mut rewards = Vec::with_capacity(len as usize);
        for _ in 0..len {
            rewards.push(cur.take_asset()?);
        }

        Some(RewardTx {
            base: tx,
            to: key,
            rewards
        })
    }
}

#[derive(Debug, Clone)]
pub struct BondTx {
    pub base: Tx,
    pub minter: PublicKey, // Key that signs blocks
    pub staker: PublicKey, // Hot wallet that receives rewards and stakes its balance
    pub stake_amt: Asset,
    pub bond_fee: Asset
}

impl EncodeTx for BondTx {
    fn encode(&self, v: &mut Vec<u8>) {
        self.encode_base(v);
        v.push_pub_key(&self.minter);
        v.push_pub_key(&self.staker);
        v.push_asset(&self.stake_amt);
        v.push_asset(&self.bond_fee);
    }
}

impl DecodeTx<BondTx> for BondTx {
    fn decode(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<BondTx> {
        assert_eq!(tx.tx_type, TxType::BOND);
        let minter = cur.take_pub_key()?;
        let staker = cur.take_pub_key()?;
        let stake_amt = cur.take_asset()?;
        let bond_fee = cur.take_asset()?;
        Some(BondTx {
            base: tx,
            minter,
            staker,
            stake_amt,
            bond_fee
        })
    }
}

#[derive(Debug, Clone)]
pub struct TransferTx {
    pub base: Tx,
    pub from: PublicKey,
    pub to: PublicKey,
    pub amount: Asset,
    pub memo: Vec<u8>
}

impl EncodeTx for TransferTx {
    fn encode(&self, v: &mut Vec<u8>) {
        self.encode_base(v);
        v.push_pub_key(&self.from);
        v.push_pub_key(&self.to);
        v.push_asset(&self.amount);
        v.push_bytes(&self.memo);
    }
}

impl DecodeTx<TransferTx> for TransferTx {
    fn decode(cur: &mut Cursor<&[u8]>, tx: Tx) -> Option<TransferTx> {
        assert_eq!(tx.tx_type, TxType::TRANSFER);
        let from = cur.take_pub_key()?;
        let to = cur.take_pub_key()?;
        let amount = cur.take_asset()?;
        let memo = cur.take_bytes()?;
        Some(TransferTx {
            base: tx,
            from,
            to,
            amount,
            memo
        })
    }
}

tx_deref!(RewardTx);
tx_deref!(BondTx);
tx_deref!(TransferTx);

tx_sign!(BondTx);
tx_sign!(TransferTx);

#[cfg(test)]
mod tests {
    use ::std::str::FromStr;
    use super::*;
    use crypto;

    macro_rules! cmp_base_tx {
        ($id:ident, $ty:expr, $ts:expr, $fee:expr) => {
            assert_eq!($id.tx_type, $ty);
            assert_eq!($id.timestamp, $ts);
            assert_eq!($id.fee.to_string(), $fee);
        }
    }

    #[test]
    fn test_encode_tx_with_sigs() {
        let to = crypto::KeyPair::gen_keypair();
        let reward_tx = TxVariant::RewardTx(RewardTx {
            base: Tx {
                tx_type: TxType::REWARD,
                timestamp: 123,
                fee: get_asset("123 GOLD"),
                signature_pairs: vec![]
            },
            to: to.0,
            rewards: vec![get_asset("1.50 GOLD"), get_asset("1.0 SILVER")]
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
                signature_pairs: vec![]
            },
            to: to.0,
            rewards: vec![get_asset("1.50 GOLD"), get_asset("1.0 SILVER")]
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
    fn test_encode_bond() {
        let minter = crypto::KeyPair::gen_keypair();
        let staker = crypto::KeyPair::gen_keypair();
        let bond_tx = BondTx {
            base: Tx {
                tx_type: TxType::BOND,
                timestamp: 1230,
                fee: get_asset("123 GOLD"),
                signature_pairs: vec![]
            },
            minter: minter.0,
            staker: staker.0,
            stake_amt: get_asset("1.0456 GOLD"),
            bond_fee: get_asset("1.00000000 GOLD")
        };

        let mut v = vec![];
        bond_tx.encode(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let base = Tx::decode_base(&mut c).unwrap();
        let dec = BondTx::decode(&mut c, base).unwrap();

        cmp_base_tx!(dec, TxType::BOND, 1230, "123 GOLD");
        assert_eq!(bond_tx.minter, dec.minter);
        assert_eq!(bond_tx.staker, dec.staker);
        assert_eq!(bond_tx.stake_amt.to_string(), dec.stake_amt.to_string());
        assert_eq!(bond_tx.bond_fee.to_string(), dec.bond_fee.to_string());
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
                signature_pairs: vec![]
            },
            from: from.0,
            to: to.0,
            amount: get_asset("1.0456 GOLD"),
            memo: Vec::from(String::from("Hello world!").as_bytes())
        };

        let mut v = vec![];
        transfer_tx.encode(&mut v);

        let mut c = Cursor::<&[u8]>::new(&v);
        let base = Tx::decode_base(&mut c).unwrap();
        let dec = TransferTx::decode(&mut c, base).unwrap();

        cmp_base_tx!(dec, TxType::TRANSFER, 1234567890, "1.23 GOLD");
        assert_eq!(transfer_tx.from, dec.from);
        assert_eq!(transfer_tx.to, dec.to);
        assert_eq!(transfer_tx.amount.to_string(), dec.amount.to_string());
        assert_eq!(transfer_tx.memo, dec.memo);
    }

    fn get_asset(s: &str) -> Asset {
        Asset::from_str(s).unwrap()
    }
}

