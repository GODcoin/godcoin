pub mod block;
mod index;
mod store;

use self::index::Indexer;
use self::block::*;

struct Blockchain {
    indexer: Indexer,
    genesis_block: Option<SignedBlock>
}
