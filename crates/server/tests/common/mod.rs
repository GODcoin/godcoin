use godcoin::{constants::MAX_TX_SIGNATURES, prelude::*};

pub mod minter;
pub use minter::*;

pub fn get_asset(asset: &str) -> Asset {
    asset.parse().unwrap()
}

pub fn create_tx_header(fee: &str) -> Tx {
    let expiry = godcoin::get_epoch_time() + 30;
    create_tx_header_with_expiry(fee, expiry)
}

pub fn create_tx_header_with_expiry(fee: &str, expiry: u64) -> Tx {
    let nonce: u32 = {
        let mut nonce = [0; 4];
        sodiumoxide::randombytes::randombytes_into(&mut nonce);
        u32::from_ne_bytes(nonce)
    };
    Tx {
        nonce,
        expiry,
        fee: fee.parse().unwrap(),
        signature_pairs: Vec::with_capacity(MAX_TX_SIGNATURES),
    }
}

pub fn check_sigs(tx: &TxVariant) -> bool {
    let txid = tx.calc_txid();
    for sig_pair in tx.sigs() {
        if !sig_pair.verify(txid.as_ref()) {
            return false;
        }
    }

    true
}
