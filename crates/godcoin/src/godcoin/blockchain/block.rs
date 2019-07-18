use crate::{
    crypto::{double_sha256, Digest, KeyPair, SigPair},
    serializer::*,
    tx::*,
};
use std::{io::Cursor, ops::Deref};

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    V0(BlockV0),
}

impl Block {
    #[inline]
    pub fn height(&self) -> u64 {
        match self {
            Block::V0(block) => block.height,
        }
    }

    #[inline]
    pub fn timestamp(&self) -> u64 {
        match self {
            Block::V0(block) => block.timestamp,
        }
    }

    #[inline]
    pub fn txs(&self) -> &[TxVariant] {
        match self {
            Block::V0(block) => &block.transactions,
        }
    }

    pub fn sign(self, key_pair: &KeyPair) -> SignedBlock {
        let buf = self.calc_hash();
        match self {
            Block::V0(_) => SignedBlock::V0(SignedBlockV0 {
                base: self,
                sig_pair: key_pair.sign(buf.as_ref()),
            }),
        }
    }

    pub fn verify_previous_hash(&self, prev_block: &Self) -> bool {
        let cur_prev_hash = match self {
            Block::V0(block) => &block.previous_hash,
        };
        cur_prev_hash == &prev_block.calc_hash()
    }

    pub fn verify_tx_merkle_root(&self) -> bool {
        match self {
            Block::V0(block) => {
                let digest = calc_tx_merkle_root(&block.transactions);
                block.tx_merkle_root == digest
            }
        }
    }

    pub fn calc_hash(&self) -> Digest {
        let mut buf = Vec::with_capacity(1024);
        self.serialize_header(&mut buf);
        double_sha256(&buf)
    }

    pub fn deserialize_with_tx(cur: &mut Cursor<&[u8]>) -> Option<Self> {
        let block_ver = cur.take_u16().ok()?;
        match block_ver {
            0 => {
                let previous_hash = Digest::from_slice(&cur.take_bytes().ok()?)?;
                let height = cur.take_u64().ok()?;
                let timestamp = cur.take_u64().ok()?;
                let tx_merkle_root = Digest::from_slice(&cur.take_bytes().ok()?)?;

                let len = cur.take_u32().ok()?;
                let mut transactions = Vec::<TxVariant>::with_capacity(len as usize);
                for _ in 0..len {
                    transactions.push(TxVariant::deserialize(cur)?);
                }

                Some(Block::V0(BlockV0 {
                    previous_hash,
                    height,
                    timestamp,
                    tx_merkle_root,
                    transactions,
                }))
            }
            _ => None,
        }
    }

    pub fn serialize_with_tx(&self, buf: &mut Vec<u8>) {
        match self {
            Block::V0(block) => {
                self.serialize_header(buf);
                buf.push_u32(block.transactions.len() as u32);
                for tx in &block.transactions {
                    tx.serialize(buf)
                }
            }
        }
    }

    fn serialize_header(&self, buf: &mut Vec<u8>) {
        match self {
            Block::V0(block) => {
                // Block version (2 bytes)
                buf.push_u16(0);

                buf.push_bytes(block.previous_hash.as_ref());
                buf.push_u64(block.height);
                buf.push_u64(block.timestamp);
                buf.push_bytes(block.tx_merkle_root.as_ref());
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockV0 {
    pub previous_hash: Digest,
    pub height: u64,
    pub timestamp: u64,
    pub tx_merkle_root: Digest,
    pub transactions: Vec<TxVariant>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SignedBlock {
    V0(SignedBlockV0),
}

impl SignedBlock {
    #[inline]
    pub fn signer(&self) -> &SigPair {
        match self {
            SignedBlock::V0(block) => &block.sig_pair,
        }
    }

    pub fn serialize_with_tx(&self, buf: &mut Vec<u8>) {
        match self {
            SignedBlock::V0(block) => {
                block.base.serialize_with_tx(buf);
                buf.push_sig_pair(&block.sig_pair);
            }
        }
    }

    pub fn deserialize_with_tx(cur: &mut Cursor<&[u8]>) -> Option<Self> {
        let block = Block::deserialize_with_tx(cur)?;
        match block {
            Block::V0(_) => {
                let sig_pair = cur.take_sig_pair().ok()?;
                Some(SignedBlock::V0(SignedBlockV0 {
                    base: block,
                    sig_pair,
                }))
            }
        }
    }
}

impl Deref for SignedBlock {
    type Target = Block;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self {
            SignedBlock::V0(block) => &block.base,
        }
    }
}

impl AsRef<Block> for SignedBlock {
    #[inline]
    fn as_ref(&self) -> &Block {
        match self {
            SignedBlock::V0(block) => &block.base,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SignedBlockV0 {
    base: Block,
    sig_pair: SigPair,
}

impl SignedBlockV0 {
    pub fn new_child(&self, txs: Vec<TxVariant>) -> Block {
        let previous_hash = self.base.calc_hash();
        let height = (match &self.base {
            Block::V0(block) => block.height,
        }) + 1;
        let tx_merkle_root = calc_tx_merkle_root(&txs);
        let timestamp = crate::get_epoch_ms();
        Block::V0(BlockV0 {
            previous_hash,
            height,
            timestamp,
            tx_merkle_root,
            transactions: txs,
        })
    }
}

pub fn calc_tx_merkle_root(txs: &[TxVariant]) -> Digest {
    let mut buf = Vec::with_capacity(4096 * txs.len());
    for tx in txs {
        tx.serialize(&mut buf)
    }
    double_sha256(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{asset::Asset, crypto::KeyPair};

    #[test]
    fn serialize_block_v0() {
        let keys = KeyPair::gen();
        let transactions = vec![TxVariant::V0(TxVariantV0::RewardTx(RewardTx {
            base: Tx {
                fee: Asset::default(),
                timestamp: 1234567890,
                signature_pairs: Vec::new(),
            },
            to: keys.0.clone().into(),
            rewards: Asset::default(),
        }))];
        let tx_merkle_root = {
            let mut buf = Vec::new();
            for tx in &transactions {
                tx.serialize(&mut buf)
            }
            double_sha256(&buf)
        };
        let block = Block::V0(BlockV0 {
            previous_hash: Digest::from_slice(&[0u8; 32]).unwrap(),
            height: 123,
            timestamp: 1532992800,
            tx_merkle_root,
            transactions,
        })
        .sign(&keys);

        let mut buf = Vec::new();
        block.serialize_with_tx(&mut buf);

        let mut cur = Cursor::<&[u8]>::new(&buf);
        let dec = SignedBlock::deserialize_with_tx(&mut cur).unwrap();

        assert_eq!(block, dec);
    }

    #[test]
    fn merkle_root() {
        let mut block = Block::V0(BlockV0 {
            previous_hash: Digest::from_slice(&[0; 32]).unwrap(),
            height: 0,
            timestamp: 0,
            tx_merkle_root: double_sha256(&[0; 0]),
            transactions: vec![],
        });
        assert!(block.verify_tx_merkle_root());

        match &mut block {
            Block::V0(block) => {
                block.tx_merkle_root = Digest::from_slice(&[0; 32]).unwrap();
            }
        }

        assert!(!block.verify_tx_merkle_root());
    }

    #[test]
    fn previous_hash() {
        let block_0 = Block::V0(BlockV0 {
            previous_hash: Digest::from_slice(&[0; 32]).unwrap(),
            height: 0,
            timestamp: 0,
            tx_merkle_root: double_sha256(&[0; 0]),
            transactions: vec![],
        });

        let block_1 = Block::V0(BlockV0 {
            previous_hash: block_0.calc_hash(),
            height: 1,
            timestamp: 0,
            tx_merkle_root: double_sha256(&[0; 0]),
            transactions: vec![],
        });

        let block_1_invalid = Block::V0(BlockV0 {
            previous_hash: Digest::from_slice(&[0; 32]).unwrap(),
            height: 1,
            timestamp: 0,
            tx_merkle_root: double_sha256(&[0; 0]),
            transactions: vec![],
        });

        assert!(block_1.verify_previous_hash(&block_0));
        assert!(!block_1_invalid.verify_previous_hash(&block_0));
    }
}
