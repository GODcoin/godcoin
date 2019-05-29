use actix::prelude::*;
use godcoin::prelude::*;
use log::{info, warn};
use std::{sync::Arc, time::Duration};

#[derive(Message)]
pub struct StartProductionLoop;

#[derive(Message)]
#[rtype(result = "Result<(), verify::TxErr>")]
pub struct PushTx(pub TxVariant);

#[derive(Message)]
#[rtype(result = "Result<(), verify::BlockErr>")]
pub struct ForceProduceBlock;

pub struct Minter {
    chain: Arc<Blockchain>,
    minter_key: KeyPair,
    tx_pool: TxPool,
}

impl Actor for Minter {
    type Context = Context<Self>;
}

impl Minter {
    pub fn new(chain: Arc<Blockchain>, minter_key: KeyPair) -> Self {
        assert_eq!(chain.get_owner().minter, minter_key.0);
        Self {
            chain: Arc::clone(&chain),
            minter_key,
            tx_pool: TxPool::new(chain),
        }
    }

    fn produce(&mut self) -> Result<(), verify::BlockErr> {
        let mut transactions = self.tx_pool.flush();

        {
            let rewards = transactions
                .iter()
                .fold(Asset::default(), |acc, tx| acc.add(tx.fee).unwrap());
            if rewards.amount > 0 {
                // Retrieve the owner wallet here in case the owner changes, ensuring that the
                // reward distribution always points to the correct address.
                let wallet_addr = self.chain.get_owner().wallet;
                transactions.push(TxVariant::RewardTx(RewardTx {
                    base: Tx {
                        tx_type: TxType::REWARD,
                        fee: Asset::default(),
                        timestamp: 0,
                        signature_pairs: Vec::new(),
                    },
                    to: wallet_addr,
                    rewards,
                }));
            }
        }

        let head = self.chain.get_chain_head();
        let block = head.new_child(transactions).sign(&self.minter_key);

        let height = block.height;
        let tx_len = block.transactions.len();

        self.chain.insert_block(block)?;
        let txs = if tx_len == 1 { "tx" } else { "txs" };
        info!(
            "Produced block at height {} with {} {}",
            height, tx_len, txs
        );
        Ok(())
    }
}

impl Handler<StartProductionLoop> for Minter {
    type Result = ();

    fn handle(&mut self, _: StartProductionLoop, ctx: &mut Self::Context) -> Self::Result {
        let dur = Duration::from_secs(3);
        ctx.run_later(dur, |minter, ctx| {
            minter.produce().unwrap();
            ctx.notify(StartProductionLoop);
        });
    }
}

impl Handler<ForceProduceBlock> for Minter {
    type Result = Result<(), verify::BlockErr>;

    fn handle(&mut self, _: ForceProduceBlock, _: &mut Self::Context) -> Self::Result {
        warn!("Forcing produced block...");
        self.produce()
    }
}

impl Handler<PushTx> for Minter {
    type Result = Result<(), verify::TxErr>;

    fn handle(&mut self, msg: PushTx, _: &mut Self::Context) -> Self::Result {
        static CONFIG: verify::Config = verify::Config::strict();
        self.tx_pool.push(msg.0.precompute(), CONFIG)
    }
}
