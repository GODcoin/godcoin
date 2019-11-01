use godcoin::prelude::{KeyPair, ScriptHash, Wif};

pub fn generate_keypair() {
    let pair = KeyPair::gen();
    println!("~~ Keys have been generated ~~");
    println!("Private key WIF: {}", pair.1.to_wif());
    println!("Public key WIF: {}", pair.0.to_wif());
    println!("P2SH key WIF: {}", ScriptHash::from(pair.0).to_wif());
    println!("- Make sure the keys are securely stored");
    println!("- Coins cannot be recovered if you lose your private key");
    println!("- Never give private keys to anyone");
}
