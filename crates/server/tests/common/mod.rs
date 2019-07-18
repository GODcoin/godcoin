use godcoin::{constants::MAX_TX_SIGNATURES, prelude::*};

pub mod minter;
pub use minter::*;

pub fn get_asset(grael: &str) -> Asset {
    grael.parse().unwrap()
}

pub fn create_tx_header(fee: &str) -> Tx {
    let timestamp = godcoin::get_epoch_ms();
    Tx {
        timestamp,
        fee: fee.parse().unwrap(),
        signature_pairs: Vec::with_capacity(MAX_TX_SIGNATURES),
    }
}

pub fn create_tx_header_with_ts(fee: &str, timestamp: u64) -> Tx {
    Tx {
        timestamp,
        fee: fee.parse().unwrap(),
        signature_pairs: Vec::with_capacity(MAX_TX_SIGNATURES),
    }
}

pub fn check_sigs(tx: &TxVariant) -> bool {
    let mut buf = Vec::with_capacity(4096);
    tx.serialize_without_sigs(&mut buf);
    for sig_pair in tx.sigs() {
        if !sig_pair.verify(&buf) {
            return false;
        }
    }

    true
}
