use std::io::{Read, Cursor, Seek, SeekFrom, Write};
use blockchain::{block::*, index::*};
use std::collections::HashMap;
use std::cell::RefCell;
use std::borrow::Cow;
use std::path::Path;
use std::fs::File;
use std::rc::Rc;
use crc32c::*;

pub struct BlockStore {
    indexer: Rc<Indexer>,

    height: u64,
    blocks: HashMap<u64, SignedBlock>,

    file: RefCell<File>,
    byte_pos_tail: u64
}

impl BlockStore {

    pub fn new(path: &Path, indexer: Rc<Indexer>) -> BlockStore {
        let (file, tail) = if !path.is_file() {
            (File::create(path).unwrap(), 0u64)
        } else {
            let f = File::open(path).unwrap();
            let m = f.metadata().unwrap();
            (File::open(path).unwrap(), m.len())
        };

        let height = indexer.get_block_byte_pos(0).unwrap_or(0);
        let mut store = BlockStore {
            indexer,

            height,
            blocks: HashMap::new(),

            file: RefCell::new(file),
            byte_pos_tail: tail
        };

        { // Initialize the cache
            let min = if height <= 100 { height } else { height - 100 };
            let max = min + 100;
            for height in min..max {
                if let Some(block) = store.get_from_disk(height) {
                    store.blocks.insert(height, block);
                } else {
                    break;
                }
            }
        }

        store
    }

    pub fn get(&self, height: u64) -> Option<Cow<'_, SignedBlock>> {
        if height > self.height { return None }
        if let Some(block) = self.blocks.get(&height) {
            Some(Cow::Borrowed(block))
        } else {
            Some(Cow::Owned(self.get_from_disk(height)?))
        }
    }

    fn get_from_disk(&self, height: u64) -> Option<SignedBlock> {
        if height > self.height { return None }

        let pos = self.indexer.get_block_byte_pos(height).unwrap();
        let mut f = self.file.borrow_mut();
        f.seek(SeekFrom::Start(pos)).unwrap();

        let (block_len, crc) = {
            let mut meta = [0u8; 8];
            f.read_exact(&mut meta).unwrap();
            let len = ((u32::from(meta[0]) << 24)
                        | (u32::from(meta[1]) << 16)
                        | (u32::from(meta[2]) << 8)
                        | u32::from(meta[3])) as usize;
            let crc = (u32::from(meta[4]) << 24)
                        | (u32::from(meta[5]) << 16)
                        | (u32::from(meta[6]) << 8)
                        | u32::from(meta[7]);
            (len, crc)
        };

        let block_vec = {
            let mut buf = Vec::with_capacity(block_len);
            unsafe { buf.set_len(block_len); }
            f.read_exact(&mut buf).unwrap();
            assert_eq!(crc, crc32c(&buf));
            buf
        };

        let mut cursor = Cursor::<&[u8]>::new(&block_vec);
        let block = SignedBlock::decode_with_tx(&mut cursor).unwrap();
        Some(block)
    }

    pub fn insert(&mut self, block: SignedBlock) {
        assert_eq!(self.height + 1, block.height, "invalid block height");
        self.insert_raw(block);
    }

    fn insert_raw(&mut self, block: SignedBlock) {
        { // Write to disk
            let vec = &mut Vec::with_capacity(1048576);
            block.encode_with_tx(vec);
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

            self.byte_pos_tail += (len as u64) + 8;
        }

        { // Update internal cache
            let height = block.height;
            self.height = height;
            let opt = self.blocks.insert(height, block);
            if self.blocks.len() > 100 {
                let b = self.blocks.remove(&(height - 100));
                debug_assert!(b.is_some(), "nothing removed from cache");
            }
            debug_assert!(opt.is_none(), "block already in the chain");
        }
    }
}
