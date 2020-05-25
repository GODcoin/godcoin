use crate::SubscriptionPool;
use godcoin::{constants::BLOCK_PROD_TIME, prelude::*};
use parking_lot::Mutex;
use std::{sync::Arc, time::Duration};
use tokio::time;
use tracing::{info, warn};

#[derive(Clone)]
pub struct Minter {
    chain: Arc<Blockchain>,
    minter_key: KeyPair,
    receipt_pool: Arc<Mutex<ReceiptPool>>,
    client_pool: SubscriptionPool,
    enable_stale_production: bool,
}

impl Minter {
    pub fn new(
        chain: Arc<Blockchain>,
        minter_key: KeyPair,
        pool: SubscriptionPool,
        enable_stale_production: bool,
    ) -> Self {
        match chain.get_owner() {
            TxVariant::V0(tx) => match tx {
                TxVariantV0::OwnerTx(tx) => assert_eq!(tx.minter, minter_key.0),
                _ => unreachable!(),
            },
        }
        Self {
            chain: Arc::clone(&chain),
            minter_key,
            receipt_pool: Arc::new(Mutex::new(ReceiptPool::new(chain))),
            client_pool: pool,
            enable_stale_production,
        }
    }

    pub fn start_production_loop(self) {
        let dur = Duration::from_secs(BLOCK_PROD_TIME);
        tokio::spawn(async move {
            // We use a delay rather than an interval to prevent mass-producing blocks if the timer needs to "catch up"
            // on missed interval events.
            time::delay_for(dur).await;
            self.produce(false).unwrap();
            self.start_production_loop();
        });
    }

    pub fn force_produce_block(
        &self,
        force_stale_production: bool,
    ) -> Result<(), blockchain::BlockErr> {
        warn!("Forcing produced block...");
        self.produce(force_stale_production)
    }

    fn produce(&self, force_stale_production: bool) -> Result<(), blockchain::BlockErr> {
        let receipts = self.receipt_pool.lock().flush();
        let should_produce =
            if force_stale_production || self.enable_stale_production || !receipts.is_empty() {
                true
            } else {
                // We don't test the current tx pool for transactions because the tip of the chain
                // should have no transactions to allow propagation finality of the previous block
                let current_head = self.chain.get_chain_head();
                !current_head.receipts().is_empty()
            };

        if !should_produce {
            let height = self.chain.get_chain_head().height();
            info!(
                "[current height: {}] No new transactions, refusing to produce block",
                height
            );
            return Ok(());
        }

        let head = self.chain.get_chain_head();
        let block = match head.as_ref() {
            Block::V0(block) => {
                let mut b = block.new_child(receipts);
                b.sign(&self.minter_key);
                b
            }
        };

        let height = block.height();
        let receipt_len = block.receipts().len();

        self.chain.insert_block(block.clone())?;
        let receipts = if receipt_len == 1 {
            "receipt"
        } else {
            "receipts"
        };
        info!(
            "Produced block at height {} with {} {}",
            height, receipt_len, receipts
        );

        self.client_pool
            .broadcast(rpc::Response::GetBlock(FilteredBlock::Block(Arc::new(
                block,
            ))));
        Ok(())
    }

    pub fn push_tx(&self, tx: TxVariant) -> Result<(), blockchain::TxErr> {
        self.receipt_pool
            .lock()
            .push(tx.precompute(), blockchain::skip_flags::SKIP_NONE)
    }

    pub fn get_account_info(&self, id: AccountId) -> Result<AccountInfo, blockchain::TxErr> {
        self.receipt_pool
            .lock()
            .get_account_info(id)
            .ok_or(blockchain::TxErr::Arithmetic)
    }
}
