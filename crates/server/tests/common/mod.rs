use godcoin::prelude::*;

pub mod minter;
pub use minter::*;

pub fn get_balance(gold: &str, silver: &str) -> Balance {
    let gold = gold.parse().unwrap();
    let silver = silver.parse().unwrap();
    Balance::from(gold, silver).unwrap()
}

pub fn create_tx(tx_type: TxType, fee: &str) -> Tx {
    let timestamp = util::get_epoch_ms();
    Tx {
        tx_type,
        timestamp,
        fee: fee.parse().unwrap(),
        signature_pairs: Vec::with_capacity(script::MAX_SIGNATURES),
    }
}

pub fn check_sigs(tx: &TxVariant) -> bool {
    let mut buf = Vec::with_capacity(4096);
    tx.serialize_without_sigs(&mut buf);
    for sig_pair in &tx.signature_pairs {
        if !sig_pair.verify(&buf) {
            return false;
        }
    }

    true
}
