use actix::prelude::*;
use godcoin::prelude::*;
use log::{info, warn};
use std::{mem, sync::Arc, time::Duration};

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
    txs: Vec<TxVariant>,
}

impl Actor for Minter {
    type Context = Context<Self>;
}

impl Minter {
    pub fn new(chain: Arc<Blockchain>, minter_key: KeyPair, wallet_addr: ScriptHash) -> Self {
        Self {
            chain,
            minter_key,
            wallet_addr,
            txs: Vec::with_capacity(1024),
        }
    }

    fn produce(&mut self) {
        let mut transactions = Vec::with_capacity(1024);
        mem::swap(&mut transactions, &mut self.txs);

        let timestamp = util::get_epoch_ms();

        transactions.push(TxVariant::RewardTx(RewardTx {
            base: Tx {
                tx_type: TxType::REWARD,
                fee: "0 GRAEL".parse().unwrap(),
                timestamp,
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
        ctx.run_interval(dur, |minter, _| {
            minter.produce();
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
        let tx = msg.0;
        self.chain
            .verify_tx(&tx, &self.txs, CONFIG)
            .map_err(TxValidateError)?;
        self.txs.push(tx);
        Ok(())
    }
}
