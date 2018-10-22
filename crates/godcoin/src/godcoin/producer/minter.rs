use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use tokio::timer::Interval;
use parking_lot::Mutex;
use tokio::prelude::*;
use std::str::FromStr;
use std::sync::Arc;

use blockchain::*;
use constants;
use crypto::*;
use asset::*;
use tx::*;

#[derive(Clone)]
pub struct Minter {
    chain: Arc<Blockchain>,
    minter: KeyPair,
    staker: PublicKey,
    txs: Arc<Mutex<Vec<TxVariant>>>
}

impl Minter {

    pub fn new(chain: Arc<Blockchain>,
                minter: KeyPair,
                staker: PublicKey) -> Self {
        Self {
            chain,
            minter,
            staker,
            txs: Arc::new(Mutex::new(Vec::with_capacity(512)))
        }
    }

    pub fn start_timer(self) {
        let dur = Duration::from_millis(constants::BLOCK_PROD_TIME);
        let at = Instant::now() + dur;
        ::tokio::spawn(Interval::new(at, dur).for_each(move |_| {
            self.produce();
            Ok(())
        }).map_err(|err| {
            error!("Timer error in minter production: {:?}", err);
        }));
    }

    fn produce(&self) {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
        let mut transactions = vec![
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
        let mut txs = self.txs.lock();
        transactions.extend_from_slice(&txs);
        txs.clear();

        let head = self.chain.get_chain_head();
        let block = head.new_child(transactions).sign(&self.minter);

        let height = block.height;
        let tx_len = block.transactions.len();

        self.chain.insert_block(block).unwrap();
        let txs = if tx_len == 1 { "tx" } else { "txs" };
        info!("Produced block at height {} with {} {}", height, tx_len, txs);
    }

    pub fn add_block(&self, block: SignedBlock) -> Result<(), String> {
        // TODO check if the block is produced too quickly
        // TODO clear the tx pool for dupe transactions
        // TODO broadcast to peers
        self.chain.insert_block(block)?;
        self.txs.lock().clear();
        Ok(())
    }

    pub fn add_tx(&self, tx: TxVariant) -> Result<(), String> {
        // TODO: verify not a duplicate tx
        // TODO: broadcast to peers
        let mut txs = self.txs.lock();
        match &tx {
            TxVariant::RewardTx(_) => {
                return Err("reward transaction not allowed".to_owned())
            },
            TxVariant::BondTx(_) => {
                self.chain.verify_tx(&tx, &txs)?;
            },
            TxVariant::TransferTx(_) => {
                self.chain.verify_tx(&tx, &txs)?;
            }
        }
        txs.push(tx);
        Ok(())
    }
}
