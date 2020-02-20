use crate::{
    asset::Asset,
    blockchain::Receipt,
    crypto::{double_sha256, Digest, DoubleSha256, KeyPair, ScriptHash, SigPair},
    serializer::*,
    tx::TxVariant,
};
use std::{collections::BTreeSet, io::Cursor, ops::Deref, sync::Arc};

pub type BlockFilter = BTreeSet<ScriptHash>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilteredBlock {
    Header((BlockHeader, SigPair)),
    Block(Arc<Block>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
    pub fn rewards(&self) -> Asset {
        match self {
            Block::V0(block) => block.rewards,
        }
    }

    #[inline]
    pub fn receipts(&self) -> &[Receipt] {
        match self {
            Block::V0(block) => &block.receipts,
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

    pub fn verify_receipt_root(&self) -> bool {
        match self {
            Block::V0(block) => {
                let digest = calc_receipt_root(&block.receipts);
                block.receipt_root == digest
            }
        }
    }

    pub fn calc_header_hash(&self) -> Digest {
        match self {
            Block::V0(block) => block.calc_header_hash(),
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
                buf.push_asset(block.rewards);
                buf.push_u32(block.receipts.len() as u32);
                for r in &block.receipts {
                    r.serialize(buf)
                }
            }
        }
    }

    pub fn deserialize(cur: &mut Cursor<&[u8]>) -> Option<Self> {
        let header = BlockHeader::deserialize(cur)?;
        match header {
            BlockHeader::V0(header) => {
                let signer = Some(cur.take_sig_pair().ok()?);
                let rewards = cur.take_asset().ok()?;

                let len = cur.take_u32().ok()?;
                let mut receipts = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    receipts.push(Receipt::deserialize(cur)?);
                }

                Some(Block::V0(BlockV0 {
                    header,
                    signer,
                    rewards,
                    receipts,
                }))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockHeaderV0 {
    pub previous_hash: Digest,
    pub height: u64,
    pub timestamp: u64,
    pub receipt_root: Digest,
}

impl BlockHeaderV0 {
    pub(self) fn serialize(&self, buf: &mut Vec<u8>) {
        // Header version (2 bytes)
        buf.push_u16(0x00);

        buf.push_digest(&self.previous_hash);
        buf.push_u64(self.height);
        buf.push_u64(self.timestamp);
        buf.push_digest(&self.receipt_root);
    }

    pub(self) fn deserialize(cur: &mut Cursor<&[u8]>) -> Option<Self> {
        // We expect the version to already be deserialized here

        let previous_hash = cur.take_digest().ok()?;
        let height = cur.take_u64().ok()?;
        let timestamp = cur.take_u64().ok()?;
        let receipt_root = cur.take_digest().ok()?;
        Some(Self {
            previous_hash,
            height,
            timestamp,
            receipt_root,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockV0 {
    pub header: BlockHeaderV0,
    pub signer: Option<SigPair>,
    pub rewards: Asset,
    pub receipts: Vec<Receipt>,
}

impl BlockV0 {
    pub fn new_child(&self, receipts: Vec<Receipt>) -> Block {
        let previous_hash = self.calc_header_hash();
        let height = self.header.height + 1;
        let receipt_root = calc_receipt_root(&receipts);
        let timestamp = crate::get_epoch_time();
        let rewards = receipts
            .iter()
            .fold(Asset::default(), |acc, receipt| match &receipt.tx {
                TxVariant::V0(tx) => acc.checked_add(tx.fee).unwrap(),
            });
        Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash,
                height,
                timestamp,
                receipt_root,
            },
            signer: None,
            rewards,
            receipts,
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

pub fn calc_receipt_root(receipts: &[Receipt]) -> Digest {
    let mut hasher = DoubleSha256::new();
    let mut buf = Vec::with_capacity(4096);
    for receipt in receipts {
        buf.clear();
        receipt.serialize(&mut buf);
        hasher.update(&buf);
    }
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{asset::Asset, crypto::KeyPair, tx::*};

    #[test]
    fn serialize_block_v0() {
        let keys = KeyPair::gen();
        let receipts = vec![Receipt {
            tx: TxVariant::V0(TxVariantV0::TransferTx(TransferTx {
                base: Tx {
                    nonce: 111,
                    expiry: 1234567890,
                    fee: Asset::default(),
                    signature_pairs: Vec::new(),
                },
                from: (&keys.0).into(),
                script: keys.0.clone().into(),
                call_fn: 0,
                args: vec![],
                amount: "1.00000 TEST".parse().unwrap(),
                memo: vec![0x00, 0x01, 0x02, 0x03],
            })),
            log: vec![],
        }];
        let receipt_root = calc_receipt_root(&receipts);
        let mut block = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash: Digest::from_slice(&[0u8; 32]).unwrap(),
                height: 123,
                timestamp: 1532992800,
                receipt_root,
            },
            signer: None,
            rewards: "1.00000 TEST".parse().unwrap(),
            receipts,
        });
        block.sign(&keys);

        let mut buf = Vec::new();
        block.serialize(&mut buf);

        let mut cur = Cursor::<&[u8]>::new(&buf);
        let dec = Block::deserialize(&mut cur).unwrap();

        assert_eq!(block, dec);
    }

    #[test]
    fn receipt_root() {
        let mut block = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash: Digest::from_slice(&[0; 32]).unwrap(),
                height: 0,
                timestamp: 0,
                receipt_root: double_sha256(&[0; 0]),
            },
            signer: None,
            rewards: Asset::default(),
            receipts: vec![],
        });
        assert!(block.verify_receipt_root());

        match &mut block {
            Block::V0(block) => {
                block.header.receipt_root = Digest::from_slice(&[0; 32]).unwrap();
            }
        }

        assert!(!block.verify_receipt_root());
    }

    #[test]
    fn previous_hash() {
        let block_0 = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash: Digest::from_slice(&[0; 32]).unwrap(),
                height: 0,
                timestamp: 0,
                receipt_root: double_sha256(&[0; 0]),
            },
            signer: None,
            rewards: Asset::default(),
            receipts: vec![],
        });

        let block_1 = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash: block_0.calc_header_hash(),
                height: 1,
                timestamp: 0,
                receipt_root: double_sha256(&[0; 0]),
            },
            signer: None,
            rewards: Asset::default(),
            receipts: vec![],
        });

        let block_1_invalid = Block::V0(BlockV0 {
            header: BlockHeaderV0 {
                previous_hash: Digest::from_slice(&[0; 32]).unwrap(),
                height: 1,
                timestamp: 0,
                receipt_root: double_sha256(&[0; 0]),
            },
            signer: None,
            rewards: Asset::default(),
            receipts: vec![],
        });

        assert!(block_1.verify_previous_hash(&block_0));
        assert!(!block_1_invalid.verify_previous_hash(&block_0));
    }
}
