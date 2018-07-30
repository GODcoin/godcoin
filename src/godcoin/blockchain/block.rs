use std::ops::Deref;
use std::io::Cursor;

use serializer::*;
use crypto;
use tx::*;

pub struct Block {
    pub previous_hash: [u8; 32],
    pub height: u64,
    pub timestamp: u32,
    pub tx_merkle_root: [u8; 32],
    pub transactions: Vec<TxVariant>
}

impl Block {
    pub fn encode(&self, vec: &mut Vec<u8>) {
        vec.push_bytes(&self.previous_hash);
        vec.push_u64(self.height);
        vec.push_u32(self.timestamp);
        vec.push_bytes(&self.tx_merkle_root);
    }

    pub fn decode(cur: &mut Cursor<&[u8]>) -> Option<Block> {
        let previous_hash = {
            let mut buf = [0u8; 32];
            let bytes = cur.take_bytes()?;
            buf.copy_from_slice(&bytes);
            buf
        };
        let height = cur.take_u64()?;
        let timestamp = cur.take_u32()?;
        let tx_merkle_root = {
            let mut buf = [0u8; 32];
            let bytes = cur.take_bytes()?;
            buf.copy_from_slice(&bytes);
            buf
        };

        let len = cur.take_u32()?;
        let mut transactions = Vec::<TxVariant>::with_capacity(len as usize);
        for _ in 0..len {
            let mut base = Tx::decode_base(cur)?;
            let var = match base.tx_type {
                TxType::REWARD => TxVariant::RewardTx(RewardTx::decode(cur, base)?),
                TxType::BOND => TxVariant::BondTx(BondTx::decode(cur, base)?),
                TxType::TRANSFER => TxVariant::TransferTx(TransferTx::decode(cur, base)?)
            };
            base.signature_pairs = crypto::SigPair::decode_from_bytes(cur)?;
            transactions.push(var);
        }

        Some(Block {
            previous_hash,
            height,
            timestamp,
            tx_merkle_root,
            transactions
        })
    }
}

pub struct SignedBlock {
    pub base: Block,
    pub sig_pair: crypto::SigPair
}

impl SignedBlock {
    pub fn encode(&self, vec: &mut Vec<u8>) {
        vec.push_bytes(&self.previous_hash);
        vec.push_u64(self.height);
        vec.push_u32(self.timestamp);
        vec.push_bytes(&self.tx_merkle_root);
    }

    pub fn decode(&self, cur: &mut Cursor<&[u8]>) -> Option<SignedBlock> {
        let block = Block::decode(cur)?;
        let sig_pair = cur.take_sig_pair()?;
        Some(SignedBlock {
            base: block,
            sig_pair
        })
    }
}

impl Deref for SignedBlock {
    type Target = Block;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
