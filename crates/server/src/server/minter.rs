use actix::prelude::*;
use godcoin::prelude::*;
use log::{info, warn};
use std::{sync::Arc, time::Duration};

#[derive(Message)]
pub struct StartProductionLoop;

#[derive(Message)]
pub struct ForceProduceBlock;

#[derive(Message)]
#[rtype(result = "Result<(), TxValidateError>")]
pub struct PushTx(pub TxVariant);

#[derive(MessageResponse)]
pub struct TxValidateError(pub verify::TxErr);

pub struct Minter {
    chain: Arc<Blockchain>,
    minter_key: KeyPair,
    wallet_addr: ScriptHash,
    tx_pool: TxPool,
}

impl Actor for Minter {
    type Context = Context<Self>;
}

impl Minter {
    pub fn new(chain: Arc<Blockchain>, minter_key: KeyPair, wallet_addr: ScriptHash) -> Self {
        Self {
            chain: Arc::clone(&chain),
            minter_key,
            wallet_addr,
            tx_pool: TxPool::new(chain),
        }
    }

    fn produce(&mut self) {
        let mut transactions = self.tx_pool.flush();

        transactions.push(TxVariant::RewardTx(RewardTx {
            base: Tx {
                tx_type: TxType::REWARD,
                fee: "0 GRAEL".parse().unwrap(),
                timestamp: 0,
                signature_pairs: Vec::new(),
            },
            to: self.wallet_addr.clone(),
            rewards: Asset::default(),
        }));

        let head = self.chain.get_chain_head();
        let block = head.new_child(transactions).sign(&self.minter_key);

        let height = block.height;
        let tx_len = block.transactions.len();

        self.chain.insert_block(block).unwrap();
        let txs = if tx_len == 1 { "tx" } else { "txs" };
        info!(
            "Produced block at height {} with {} {}",
            height, tx_len, txs
        );
    }
}

impl Handler<StartProductionLoop> for Minter {
    type Result = ();

    fn handle(&mut self, _: StartProductionLoop, ctx: &mut Self::Context) -> Self::Result {
        let dur = Duration::from_secs(3);
        ctx.run_later(dur, |minter, ctx| {
            minter.produce();
            ctx.notify(StartProductionLoop);
        });
    }
}

impl Handler<ForceProduceBlock> for Minter {
    type Result = ();

    fn handle(&mut self, _: ForceProduceBlock, _: &mut Self::Context) -> Self::Result {
        warn!("Forcing produced block...");
        self.produce();
    }
}

impl Handler<PushTx> for Minter {
    type Result = Result<(), TxValidateError>;

    fn handle(&mut self, msg: PushTx, _: &mut Self::Context) -> Self::Result {
        static CONFIG: verify::Config = verify::Config::strict();
        self.tx_pool.push(msg.0, CONFIG).map_err(TxValidateError)
    }
}
