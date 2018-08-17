use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use tokio::timer::Interval;
use tokio::prelude::*;
use std::str::FromStr;
use std::sync::Arc;

use blockchain::*;
use crypto::*;
use asset::*;
use tx::*;

pub struct Producer {
    chain: Arc<Blockchain>,
    minter: KeyPair,
    staker: PublicKey
}

impl Producer {

    pub fn new(chain: Arc<Blockchain>, minter: KeyPair, staker: PublicKey) -> Producer {
        Producer {
            chain,
            minter,
            staker
        }
    }

    pub fn start_timer(self) {
        let dur = Duration::from_secs(3);
        let at = Instant::now() + dur;
        ::tokio::spawn(Interval::new(at, dur).take_while(|_| {
            Ok(true)
        }).for_each(move |_| {
            Producer::produce(&self);
            Ok(())
        }).map_err(|err| {
            println!("Unknown error in producer: {:?}", err);
        }));
    }

    fn produce(&self) {
        let blockchain = &*self.chain;

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
        let transactions = {
            let mut vec = Vec::new();
            vec.push(TxVariant::RewardTx(RewardTx {
                base: Tx {
                    tx_type: TxType::REWARD,
                    fee: Asset::from_str("0 GOLD").unwrap(),
                    timestamp,
                    signature_pairs: Vec::new()
                },
                to: self.staker.clone(),
                rewards: vec![Asset::from_str("1 GOLD").unwrap()]
            }));
            vec
        };

        let head = blockchain.get_chain_head();
        let block = head.new_child(transactions).sign(&self.minter);
        blockchain.insert_block(block);
    }
}
