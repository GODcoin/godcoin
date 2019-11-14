use crc32c::*;
use log::{debug, error, warn};
use std::{
    cell::RefCell,
    collections::HashMap,
    convert::TryInto,
    fs::{File, OpenOptions},
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::Arc,
};

use crate::blockchain::{block::*, index::*};

const MAX_CACHE_SIZE: u64 = 100;

#[derive(Clone, Debug, PartialEq)]
pub struct ReindexOpts {
    pub auto_trim: bool,
}

pub struct BlockStore {
    indexer: Arc<Indexer>,

    height: u64,
    blocks: HashMap<u64, Arc<Block>>,
    genesis_block: Option<Arc<Block>>,

    file: RefCell<File>,
    byte_pos_tail: u64,
}

impl BlockStore {
    pub fn new(blocklog_file: &Path, indexer: Arc<Indexer>) -> BlockStore {
        let (file, tail) = {
            let f = OpenOptions::new()
                .create(true)
                .read(true)
                .append(true)
                .open(blocklog_file)
                .unwrap();
            let m = f.metadata().unwrap();
            (f, m.len())
        };

        let mut store = BlockStore {
            indexer,

            height: 0,
            blocks: HashMap::new(),
            genesis_block: None,

            file: RefCell::new(file),
            byte_pos_tail: tail,
        };

        store.init_state();
        store
    }

    #[inline(always)]
    pub fn get_chain_height(&self) -> u64 {
        self.height
    }

    pub fn get(&self, height: u64) -> Option<Arc<Block>> {
        if height > self.height {
            return None;
        } else if height == 0 {
            if let Some(ref block) = self.genesis_block {
                return Some(Arc::clone(block));
            }
        }
        if let Some(block) = self.blocks.get(&height) {
            Some(Arc::clone(block))
        } else {
            Some(Arc::new(self.read_from_disk(height)?))
        }
    }

    pub fn is_empty(&self) -> bool {
        let meta = self.file.borrow().metadata().unwrap();
        meta.len() == 0
    }

    pub fn insert(&mut self, batch: &mut WriteBatch, block: Block) {
        assert_eq!(self.height + 1, block.height(), "invalid block height");
        let byte_pos = self.byte_pos_tail;
        self.write_to_disk(&block);

        // Update internal cache
        let height = block.height();
        self.height = height;
        batch.set_block_byte_pos(height, byte_pos);
        batch.set_chain_height(height);

        let opt = self.blocks.insert(height, Arc::new(block));
        debug_assert!(opt.is_none(), "block already in the chain");

        if self.blocks.len() > MAX_CACHE_SIZE as usize {
            let b = self.blocks.remove(&(height - MAX_CACHE_SIZE));
            debug_assert!(b.is_some(), "nothing removed from cache");
        }
    }

    pub fn insert_genesis(&mut self, batch: &mut WriteBatch, block: Block) {
        assert_eq!(block.height(), 0, "expected to be 0");
        assert!(
            self.genesis_block.is_none(),
            "expected genesis block to not exist"
        );
        assert!(self.is_empty(), "block log must be empty");
        self.write_to_disk(&block);
        self.genesis_block = Some(Arc::new(block));
        batch.set_block_byte_pos(0, 0);
    }

    pub fn reindex_blocks<F>(&mut self, opts: ReindexOpts, index_fn: F)
    where
        F: Fn(&mut WriteBatch, &Block),
    {
        let mut batch = WriteBatch::new(Arc::clone(&self.indexer));
        let mut last_known_good_height = 0;
        let mut pos = 0;
        loop {
            match self.raw_read_from_disk(pos) {
                Ok(block) => {
                    let height = block.height();
                    let new_pos = {
                        let mut f = self.file.borrow_mut();
                        f.seek(SeekFrom::Current(0)).unwrap()
                    };
                    if !(last_known_good_height == 0 || height == last_known_good_height + 1) {
                        error!("Invalid height ({}) detected at byte pos {}", height, pos);
                        if opts.auto_trim {
                            warn!("Truncating block log");
                            let f = self.file.borrow();
                            f.set_len(pos).unwrap();
                            self.byte_pos_tail = pos;
                        } else {
                            panic!("corruption detected, auto trim is disabled");
                        }
                        break;
                    }

                    batch.set_block_byte_pos(height, pos);
                    batch.set_chain_height(height);
                    index_fn(&mut batch, &block);
                    debug!("Reindexed block {} at pos {}", height, pos);

                    pos = new_pos;
                    last_known_good_height = height;
                }
                Err(e) => match e {
                    ReadError::Eof => break,
                    ReadError::CorruptBlock => {
                        error!(
                            "(last known good height: {}, block end byte pos: {})",
                            last_known_good_height, pos
                        );
                        if opts.auto_trim {
                            warn!("Truncating block log");
                            let f = self.file.borrow();
                            f.set_len(pos).unwrap();
                            self.byte_pos_tail = pos;
                            break;
                        } else {
                            panic!("corrupt block detected, auto trim is disabled");
                        }
                    }
                },
            }
        }

        batch.commit();
        self.indexer.set_index_status(IndexStatus::Complete);
        self.init_state();
    }

    fn read_from_disk(&self, height: u64) -> Option<Block> {
        if height > self.height {
            return None;
        }

        let pos = self.indexer.get_block_byte_pos(height)?;
        self.raw_read_from_disk(pos).ok()
    }

    fn raw_read_from_disk(&self, pos: u64) -> Result<Block, ReadError> {
        let mut f = self.file.borrow_mut();
        f.seek(SeekFrom::Start(pos)).unwrap();

        let (block_len, crc) = {
            let mut meta = [0u8; 8];
            f.read_exact(&mut meta).map_err(|_| ReadError::Eof)?;
            let (len_buf, crc_buf) = meta.split_at(4);
            let len = u32::from_be_bytes(len_buf.try_into().unwrap()) as usize;
            let crc = u32::from_be_bytes(crc_buf.try_into().unwrap());
            (len, crc)
        };

        let block_vec = {
            let mut buf = Vec::with_capacity(block_len);
            unsafe {
                buf.set_len(block_len);
            }
            f.read_exact(&mut buf)
                .map_err(|_| ReadError::CorruptBlock)?;
            assert_eq!(crc, crc32c(&buf));
            buf
        };

        let mut cursor = Cursor::<&[u8]>::new(&block_vec);
        Block::deserialize(&mut cursor).ok_or(ReadError::CorruptBlock)
    }

    fn write_to_disk(&mut self, block: &Block) {
        let vec = &mut Vec::with_capacity(1_048_576);
        block.serialize(vec);
        let len = vec.len() as u32;
        let crc = crc32c(vec);

        let mut f = self.file.borrow_mut();
        {
            let mut buf = [0u8; 8];
            buf[0] = (len >> 24) as u8;
            buf[1] = (len >> 16) as u8;
            buf[2] = (len >> 8) as u8;
            buf[3] = len as u8;

            buf[4] = (crc >> 24) as u8;
            buf[5] = (crc >> 16) as u8;
            buf[6] = (crc >> 8) as u8;
            buf[7] = crc as u8;

            f.write_all(&buf).unwrap();
        }

        f.write_all(vec).unwrap();
        f.flush().unwrap();

        if log::log_enabled!(log::Level::Debug) {
            debug!(
                "[height:{}] Wrote {} bytes to the block log",
                block.height(),
                u64::from(len) + 8
            );
        }

        self.byte_pos_tail += u64::from(len) + 8;
    }

    fn init_state(&mut self) {
        self.height = self.indexer.get_chain_height();
        self.genesis_block = self.get(0);
        if !self.is_empty() && self.indexer.index_status() == IndexStatus::Complete {
            // Init block cache
            self.blocks.clear();
            let max = self.height;
            let min = max.saturating_sub(MAX_CACHE_SIZE);
            for height in min..=max {
                let block = self
                    .read_from_disk(height)
                    .unwrap_or_else(|| panic!("Failed to read block {} from disk", height));
                self.blocks.insert(height, Arc::new(block));
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum ReadError {
    Eof,
    CorruptBlock,
}
