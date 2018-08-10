use std::cell::RefCell;
use std::path::*;
use std::rc::Rc;

pub mod block;
pub mod index;
pub mod store;

pub use self::store::BlockStore;
pub use self::index::Indexer;
pub use self::block::*;

pub struct Blockchain {
    pub indexer: Rc<Indexer>,
    pub genesis_block: Option<SignedBlock>,
    pub store: RefCell<BlockStore>
}

impl Blockchain {

    ///
    /// Creates a new `Blockchain` with an associated indexer and backing
    /// storage is automatically created based on the given `path`.
    ///
    pub fn new(path: &Path) -> Blockchain {
        let indexer = Rc::new(Indexer::new(&Path::join(path, "index")));
        let store = BlockStore::new(&Path::join(path, "blklog"), Rc::clone(&indexer));
        let genesis_block = store.get(0).map(|b| b.into_owned());
        Blockchain {
            indexer,
            genesis_block,
            store: RefCell::new(store)
        }
    }
}
