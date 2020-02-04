use futures::{
    task::{Context, Poll},
    Stream,
};
use godcoin::prelude::{BlockFilter, Blockchain, FilteredBlock};
use std::{pin::Pin, sync::Arc};

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

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<Self::Item>> {
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
            Poll::Ready(Some(block))
        } else {
            Poll::Ready(None)
        }
    }
}
