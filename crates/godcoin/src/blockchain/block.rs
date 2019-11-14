use crate::{
    crypto::{double_sha256, Digest, KeyPair, ScriptHash, SigPair},
    serializer::*,
    tx::*,
};
use std::{collections::BTreeSet, io::Cursor, ops::Deref, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub enum BlockFilter {
    /// No filter applied
    None,
    /// Filters block based on funds being transferred from or to an address. Some nodes may treat a request with
    /// too many addresses as invalid.
    Addr(BTreeSet<ScriptHash>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum FilteredBlock {
    Header((BlockHeader, SigPair)),
    Block(Arc<Block>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    V0(BlockV0),
}

impl Block {
    #[inline]
    pub fn header(&self) -> BlockHeader {
        match self {
            Block::V0(block) => BlockHeader::V0(block.header.clone()),
        }
    }

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

    #[inline]
    pub fn signer(&self) -> Option<&SigPair> {
        match self {
            Block::V0(block) => block.signer.as_ref(),
        }
    }

    pub fn sign(&mut self, key_pair: &KeyPair) {
        let buf = self.calc_header_hash();
        match self {
            Block::V0(block) => {
                block.signer = Some(key_pair.sign(buf.as_ref()));
            }
        }
    }

    pub fn verify_previous_hash(&self, prev_block: &Self) -> bool {
        let cur_prev_hash = match self {
            Block::V0(block) => &block.previous_hash,
        };
        cur_prev_hash == &prev_block.calc_header_hash()
    }

    pub fn verify_tx_merkle_root(&self) -> bool {
        match self {
            Block::V0(block) => {
                let digest = calc_tx_merkle_root(&block.transactions);
                block.tx_merkle_root == digest
            }
        }
    }

    pub fn calc_header_hash(&self) -> Digest {
        match self {
            Block::V0(block) => block.calc_header_hash(),
        }
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> Option<Self> {
        let header = BlockHeader::deserialize(cur)?;
        match header {
            BlockHeader::V0(header) => {
                let signer = Some(cur.take_sig_pair().ok()?);

                let len = cur.take_u32().ok()?;
                let mut transactions = Vec::<TxVariant>::with_capacity(len as usize);
                for _ in 0..len {
                    transactions.push(TxVariant::deserialize(cur)?);
                }

                Some(Block::V0(BlockV0 {
                    header,
                    signer,
                    transactions,
                }))
            }
        }
    }

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            Block::V0(block) => {
                block.header.serialize(buf);
                buf.push_sig_pair(
                    block
                        .signer
                        .as_ref()
                        .expect("block must be signed to serialize"),
                );
                buf.push_u32(block.transactions.len() as u32);
                for tx in &block.transactions {
                    tx.serialize(buf)
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlockHeader {
    V0(BlockHeaderV0),
}

impl BlockHeader {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            BlockHeader::V0(header) => header.serialize(buf),
        }
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> Option<Self> {
        let header_ver = cur.take_u16().ok()?;
        match header_ver {
            0x00 => Some(BlockHeader::V0(BlockHeaderV0::deserialize(cur)?)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockHeaderV0 {
    pub previous_hash: Digest,
    pub height: u64,
    pub timestamp: u64,
    pub tx_merkle_root: Digest,
}

impl BlockHeaderV0 {
    pub(self) fn serialize(&self, buf: &mut Vec<u8>) {
        // Header version (2 bytes)
        buf.push_u16(0x00);

        buf.push_digest(&self.previous_hash);
        buf.push_u64(self.height);
        buf.push_u64(self.timestamp);
        buf.push_digest(&self.tx_merkle_root);
    }

    pub(self) fn deserialize(cur: &mut Cursor<&[u8]>) -> Option<Self> {
        // We expect the version to already be deserialized here

        let previous_hash = cur.take_digest().ok()?;
        let height = cur.take_u64().ok()?;
        let timestamp = cur.take_u64().ok()?;
        let tx_merkle_root = cur.take_digest().ok()?;
        Some(Self {
            previous_hash,
            height,
            timestamp,
            tx_merkle_root,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockV0 {
    pub header: BlockHeaderV0,
    pub signer: Option<SigPair>,
    pub transactions: Vec<TxVariant>,
}

impl BlockV0 {
    pub fn new_child(&self, txs: Vec<TxVariant>) -> Block {
        let previous_hash = self.calc_header_hash();
        let height = self.header.height + 1;
        let tx_merkle_root = calc_tx_merkle_root(&txs);
        let timestamp = crate::get_epoch_ms();
        Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash,
                height,
                timestamp,
                tx_merkle_root,
            },
            signer: None,
            transactions: txs,
        })
    }

    pub fn calc_header_hash(&self) -> Digest {
        let mut buf = Vec::with_capacity(1024);
        self.header.serialize(&mut buf);
        double_sha256(&buf)
    }
}

impl Deref for BlockV0 {
    type Target = BlockHeaderV0;

    fn deref(&self) -> &BlockHeaderV0 {
        &self.header
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
        let mut block = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash: Digest::from_slice(&[0u8; 32]).unwrap(),
                height: 123,
                timestamp: 1532992800,
                tx_merkle_root,
            },
            signer: None,
            transactions,
        });
        block.sign(&keys);

        let mut buf = Vec::new();
        block.serialize(&mut buf);

        let mut cur = Cursor::<&[u8]>::new(&buf);
        let dec = Block::deserialize(&mut cur).unwrap();

        assert_eq!(block, dec);
    }

    #[test]
    fn merkle_root() {
        let mut block = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash: Digest::from_slice(&[0; 32]).unwrap(),
                height: 0,
                timestamp: 0,
                tx_merkle_root: double_sha256(&[0; 0]),
            },
            signer: None,
            transactions: vec![],
        });
        assert!(block.verify_tx_merkle_root());

        match &mut block {
            Block::V0(block) => {
                block.header.tx_merkle_root = Digest::from_slice(&[0; 32]).unwrap();
            }
        }

        assert!(!block.verify_tx_merkle_root());
    }

    #[test]
    fn previous_hash() {
        let block_0 = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash: Digest::from_slice(&[0; 32]).unwrap(),
                height: 0,
                timestamp: 0,
                tx_merkle_root: double_sha256(&[0; 0]),
            },
            signer: None,
            transactions: vec![],
        });

        let block_1 = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash: block_0.calc_header_hash(),
                height: 1,
                timestamp: 0,
                tx_merkle_root: double_sha256(&[0; 0]),
            },
            signer: None,
            transactions: vec![],
        });

        let block_1_invalid = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash: Digest::from_slice(&[0; 32]).unwrap(),
                height: 1,
                timestamp: 0,
                tx_merkle_root: double_sha256(&[0; 0]),
            },
            signer: None,
            transactions: vec![],
        });

        assert!(block_1.verify_previous_hash(&block_0));
        assert!(!block_1_invalid.verify_previous_hash(&block_0));
    }
}
