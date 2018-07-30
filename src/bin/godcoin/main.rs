extern crate godcoin;
extern crate clap;

use clap::{App, AppSettings, SubCommand};

fn generate_keypair() {
    use godcoin::*;
    let pair = KeyPair::gen_keypair();
    println!("~~ Keys have been generated ~~");
    println!("Private key WIF: {}", pair.1.to_wif());
    println!("Public key WIF: {}", pair.0.to_wif());
    println!("- Make sure the keys are securely stored");
    println!("- Coins cannot be recovered if you lose your private key");
    println!("- Never give private keys to anyone");
}

fn main() {
    godcoin::init().unwrap();

    let app = App::new("godcoin")
                .about("GODcoin core CLI")
                .version(env!("CARGO_PKG_VERSION"))
                .setting(AppSettings::VersionlessSubcommands)
                .setting(AppSettings::SubcommandRequiredElseHelp)
                .subcommand(SubCommand::with_name("keygen")
                            .help("Generates a keypair"));
    let matches = app.get_matches();

    if let Some(_) = matches.subcommand_matches("keygen") {
        generate_keypair();
    }
}
