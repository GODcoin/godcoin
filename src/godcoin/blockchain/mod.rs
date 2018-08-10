pub mod block;
pub mod index;
pub mod store;

pub use self::store::BlockStore;
pub use self::index::Indexer;
pub use self::block::*;

pub struct Blockchain<'a> {
    pub indexer: &'a Indexer,
    pub genesis_block: Option<SignedBlock>,
    pub store: &'a BlockStore<'a>
}

impl<'a> Blockchain<'a> {
    pub fn new<'b>(store: &'b BlockStore, indexer: &'b Indexer) -> Blockchain<'b> {
        Blockchain {
            indexer,
            genesis_block: store.get(0).map(|b| b.into_owned()),
            store
        }
    }
}
