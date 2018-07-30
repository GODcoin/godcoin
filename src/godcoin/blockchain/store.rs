use std::collections::HashMap;
use blockchain::block::*;
use std::path::Path;

struct BlockStore {
    height: u64,
    blocks: HashMap<u64, SignedBlock>,
    byte_pos_tail: u64
}

impl BlockStore {
    pub fn new(path: &Path) -> BlockStore {
        unimplemented!()
    }

    pub fn get(&self, height: u64) -> Option<&SignedBlock> {
        if height > self.height { return None }
        if let Some(block) = self.blocks.get(&height) {
            Some(block)
        } else {
            // TODO: retrieve from the disk cache
            None
        }
    }

    pub fn append(&mut self, block: SignedBlock) {
        // TODO: index the blocks
        self.height = block.height;
        let opt = self.blocks.insert(block.height, block);
        debug_assert!(opt.is_none());
    }
}
