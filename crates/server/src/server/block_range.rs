use futures::{Async, Poll, Stream};
use godcoin::prelude::{BlockFilter, Blockchain, FilteredBlock};
use std::sync::Arc;

pub struct AsyncBlockRange {
    chain: Arc<Blockchain>,
    filter: Option<BlockFilter>,
    min_height: u64,
    max_height: u64,
}

impl AsyncBlockRange {
    pub fn try_new(chain: Arc<Blockchain>, min_height: u64, max_height: u64) -> Option<Self> {
        if min_height > max_height || max_height > chain.get_chain_height() {
            None
        } else {
            Some(AsyncBlockRange {
                chain,
                filter: None,
                min_height,
                max_height,
            })
        }
    }

    pub fn set_filter(&mut self, filter: Option<BlockFilter>) {
        self.filter = filter;
    }
}

impl Stream for AsyncBlockRange {
    type Item = FilteredBlock;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.min_height <= self.max_height {
            let block = match self.filter {
                Some(ref filter) => self
                    .chain
                    .get_filtered_block(self.min_height, filter)
                    .unwrap_or_else(|| unreachable!()),
                None => FilteredBlock::Block(
                    self.chain
                        .get_block(self.min_height)
                        .unwrap_or_else(|| unreachable!()),
                ),
            };
            self.min_height += 1;
            Ok(Async::Ready(Some(block)))
        } else {
            Ok(Async::Ready(None))
        }
    }
}
