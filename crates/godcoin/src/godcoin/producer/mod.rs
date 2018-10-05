use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use tokio::timer::Interval;
use tokio::prelude::*;
use std::str::FromStr;
use std::sync::Arc;

use blockchain::*;
use constants;
use crypto::*;
use asset::*;
use tx::*;

pub struct Producer {
    chain: Arc<Blockchain>,
    minter: KeyPair,
    staker: PublicKey
}

impl Producer {

    pub fn new(chain: Arc<Blockchain>,
                minter: KeyPair,
                staker: PublicKey) -> Producer {
        Producer {
            chain,
            minter,
            staker
        }
    }

    pub fn start_timer(self) {
        let dur = Duration::from_millis(constants::BLOCK_PROD_TIME);
        let at = Instant::now() + dur;
        ::tokio::spawn(Interval::new(at, dur).for_each(move |_| {
            self.produce();
            Ok(())
        }).map_err(|err| {
            error!("Timer error in producer: {:?}", err);
        }));
    }

    fn produce(&self) {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
        let transactions = vec![
            TxVariant::RewardTx(RewardTx {
                base: Tx {
                    tx_type: TxType::REWARD,
                    fee: Asset::from_str("0 GOLD").unwrap(),
                    timestamp,
                    signature_pairs: Vec::new()
                },
                to: self.staker.clone(),
                rewards: vec![
                    Asset::from_str("1 GOLD").unwrap(),
                    Asset::from_str("100 SILVER").unwrap()
                ]
            })
        ];

        let head = self.chain.get_chain_head();
        let block = head.new_child(transactions).sign(&self.minter);

        let height = block.height;
        let tx_len = block.transactions.len();

        self.chain.insert_block(block).unwrap();
        info!("Produced block at height {} with {} txs", height, tx_len);
    }
}
