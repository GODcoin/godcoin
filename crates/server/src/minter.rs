use actix::prelude::*;
use godcoin::prelude::*;
use log::info;
use std::{
    mem,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct Minter {
    chain: Blockchain,
    minter_key: KeyPair,
    wallet_key: PublicKey,
    txs: Vec<TxVariant>,
}

impl Actor for Minter {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let dur = Duration::from_secs(3);
        ctx.run_interval(dur, |minter, _| {
            minter.produce();
        });
    }
}

impl Minter {
    pub fn new(chain: Blockchain, minter_key: KeyPair, wallet_key: PublicKey) -> Self {
        Self {
            chain,
            minter_key,
            wallet_key,
            txs: Vec::new(),
        }
    }

    fn produce(&mut self) {
        let mut transactions = vec![];
        mem::swap(&mut transactions, &mut self.txs);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        transactions.push(TxVariant::RewardTx(RewardTx {
            base: Tx {
                tx_type: TxType::REWARD,
                fee: "0 GOLD".parse().unwrap(),
                timestamp,
                signature_pairs: Vec::new(),
            },
            to: (&self.wallet_key).into(),
            rewards: vec![],
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
