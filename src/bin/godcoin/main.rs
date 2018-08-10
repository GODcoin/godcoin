extern crate sodiumoxide;
extern crate num_traits;
extern crate godcoin;
extern crate tokio;
extern crate dirs;
extern crate clap;

use clap::{Arg, App, AppSettings, SubCommand};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::prelude::*;
use godcoin::*;

struct StartNode<'a> {
    bind_address: Option<&'a str>,
    minter_key: Option<KeyPair>
}

fn generate_keypair() {
    let pair = KeyPair::gen_keypair();
    println!("~~ Keys have been generated ~~");
    println!("Private key WIF: {}", pair.1.to_wif());
    println!("Public key WIF: {}", pair.0.to_wif());
    println!("- Make sure the keys are securely stored");
    println!("- Coins cannot be recovered if you lose your private key");
    println!("- Never give private keys to anyone");
}

fn start_node(node_opts: StartNode) {
    use std::{env, path::Path, path::PathBuf, str::FromStr};
    use sodiumoxide::crypto::hash::sha256::Digest;
    use godcoin::blockchain::*;

    let home: PathBuf = {
        use dirs;
        let home = env::var("GODCOIN_HOME").map(|s| {
            PathBuf::from(s)
        }).unwrap_or_else(|_| {
            Path::join(&dirs::data_local_dir().unwrap(), "godcoin")
        });
        if !Path::is_dir(&home) {
            let res = std::fs::create_dir(&home);
            res.expect(&format!("Failed to create dir at {:?}", &home));
            println!("Created GODcoin home at {:?}", &home);
        } else {
            println!("Found GODcoin home at {:?}", &home);
        }
        home
    }.canonicalize().unwrap();

    let indexer = Indexer::new(&Path::join(&home, "index"));
    let store = BlockStore::new(&Path::join(&home, "blklog"), &indexer);
    let mut blockchain = Blockchain::new(&store, &indexer);

    if blockchain.genesis_block.is_none() {
        println!("=> Generating new block chain");
        let keys = crypto::KeyPair::gen_keypair();
        println!("=> Staker private key: {}", keys.1.to_wif());
        println!("=> Staker public key: {}", keys.0.to_wif());

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
                to: keys.0.clone(),
                rewards: vec![Asset::from_str("1 GOLD").unwrap()]
            }));
            vec.push(TxVariant::BondTx(BondTx {
                base: Tx {
                    tx_type: TxType::BOND,
                    fee: Asset::from_str("0 GOLD").unwrap(),
                    timestamp,
                    signature_pairs: Vec::new()
                },
                minter: keys.0.clone(),
                staker: keys.0.clone(),
                bond_fee: EMPTY_GOLD,
                stake_amt: Asset::from_str("1 GOLD").unwrap()
            }));
            vec
        };

        let minter = node_opts.minter_key.expect("missing minter key to generate new block chain");
        let block = (Block {
            height: 0,
            previous_hash: Digest::from_slice(&[0u8; 32]).unwrap(),
            tx_merkle_root: Digest::from_slice(&[0u8; 32]).unwrap(),
            timestamp: timestamp as u32,
            transactions
        }).sign(&minter);

        blockchain.genesis_block = Some(block);
    }

    if let Some(bind) = node_opts.bind_address {
        let addr = bind.parse()
                        .expect(&format!("Failed to parse address: {:?}", bind));
        net::start_server(&addr);
    }
}

fn main() {
    godcoin::init().unwrap();

    let app = App::new("godcoin")
                .about("GODcoin core CLI")
                .version(env!("CARGO_PKG_VERSION"))
                .setting(AppSettings::VersionlessSubcommands)
                .setting(AppSettings::SubcommandRequiredElseHelp)
                .subcommand(SubCommand::with_name("keygen")
                            .about("Generates a keypair"))
                .subcommand(SubCommand::with_name("node")
                            .about("Starts the blockchain node service")
                            .arg(Arg::with_name("bind_address")
                                .help("Bind address endpoint (i.e 0.0.0.0:7777)")
                                .long("bind")
                                .value_name("address"))
                            .arg(Arg::with_name("minter_key")
                                .help("Private minting key required to mint")
                                .long("minter-key")
                                .value_name("key")));
    let matches = app.get_matches();

    ::tokio::run(future::lazy(move || {
        use ::std::io::{Error, ErrorKind};

        if let Some(_) = matches.subcommand_matches("keygen") {
            generate_keypair();
        } else if let Some(matches) = matches.subcommand_matches("node") {
            start_node(StartNode {
                bind_address: matches.value_of("bind_address"),
                minter_key: matches.value_of("minter_key").map(|s| {
                    godcoin::PrivateKey::from_wif(s)
                        .expect("Failed to parse minter key argument")
                })
            });
        } else {
            return Err(Error::new(ErrorKind::Other, "Failed to match subcommand"))
        }

        Ok(())
    }).map_err(|err| {
        println!("Startup failure: {:?}", err);
    }));
}
