use crate::SubscriptionPool;
use godcoin::prelude::*;
use log::{info, warn};
use parking_lot::Mutex;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{prelude::*, timer::Delay};

#[derive(Clone)]
pub struct Minter {
    chain: Arc<Blockchain>,
    minter_key: KeyPair,
    tx_pool: Arc<Mutex<TxPool>>,
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
            tx_pool: Arc::new(Mutex::new(TxPool::new(chain))),
            client_pool: pool,
            enable_stale_production,
        }
    }

    pub fn start_production_loop(self) {
        let dur = Duration::from_secs(3);
        tokio::spawn(
            Delay::new(Instant::now() + dur)
                .and_then(move |_| {
                    self.produce(false).unwrap();
                    self.start_production_loop();
                    Ok(())
                })
                .map_err(|e| {
                    panic!("Minter production timer error: {:?}", e);
                }),
        );
    }

    pub fn force_produce_block(
        &self,
        force_stale_production: bool,
    ) -> Result<(), verify::BlockErr> {
        warn!("Forcing produced block...");
        self.produce(force_stale_production)
    }

    fn produce(&self, force_stale_production: bool) -> Result<(), verify::BlockErr> {
        let mut transactions = self.tx_pool.lock().flush();
        let should_produce =
            if force_stale_production || self.enable_stale_production || !transactions.is_empty() {
                true
            } else {
                // We don't test the current tx pool for transactions because the tip of the chain
                // should have no transactions to allow propagation finality of the previous block
                let current_head = self.chain.get_chain_head();
                !current_head.txs().is_empty()
            };

        if !should_produce {
            let height = self.chain.get_chain_head().height();
            info!(
                "[current height: {}] No new transactions, refusing to produce block",
                height
            );
            return Ok(());
        }

        {
            let rewards = transactions
                .iter()
                .fold(Asset::default(), |acc, tx| match tx {
                    TxVariant::V0(tx) => acc.checked_add(tx.fee).unwrap(),
                });
            if rewards.amount > 0 {
                // Retrieve the owner wallet here in case the owner changes, ensuring that the
                // reward distribution always points to the correct address.
                let wallet_addr = match self.chain.get_owner() {
                    TxVariant::V0(tx) => match tx {
                        TxVariantV0::OwnerTx(owner) => owner.wallet,
                        _ => unreachable!(),
                    },
                };
                transactions.push(TxVariant::V0(TxVariantV0::RewardTx(RewardTx {
                    base: Tx {
                        expiry: 0,
                        fee: Asset::default(),
                        signature_pairs: Vec::new(),
                    },
                    to: wallet_addr,
                    rewards,
                })));
            }
        }

        let head = self.chain.get_chain_head();
        let block = match head.as_ref() {
            Block::V0(block) => {
                let mut b = block.new_child(transactions);
                b.sign(&self.minter_key);
                b
            }
        };

        let height = block.height();
        let tx_len = block.txs().len();

        self.chain.insert_block(block.clone())?;
        let txs = if tx_len == 1 { "tx" } else { "txs" };
        info!(
            "Produced block at height {} with {} {}",
            height, tx_len, txs
        );

        self.client_pool
            .broadcast(rpc::Response::GetBlock(FilteredBlock::Block(Arc::new(
                block,
            ))));
        Ok(())
    }

    pub fn push_tx(&self, tx: TxVariant) -> Result<(), verify::TxErr> {
        self.tx_pool.lock().push(tx.precompute(), verify::SKIP_NONE)
    }

    pub fn get_addr_info(&self, addr: &ScriptHash) -> Result<AddressInfo, verify::TxErr> {
        self.tx_pool
            .lock()
            .get_address_info(addr)
            .ok_or(verify::TxErr::Arithmetic)
    }
}
