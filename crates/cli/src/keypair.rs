use godcoin::*;
use log::info;
use std::sync::mpsc;

pub fn generate_keypair(shutdown_handle: &mpsc::Sender<()>) {
    let pair = KeyPair::gen_keypair();
    info!("~~ Keys have been generated ~~");
    info!("Private key WIF: {}", pair.1.to_wif());
    info!("Public key WIF: {}", pair.0.to_wif());
    info!("- Make sure the keys are securely stored");
    info!("- Coins cannot be recovered if you lose your private key");
    info!("- Never give private keys to anyone");
    shutdown_handle.send(()).unwrap();
}
