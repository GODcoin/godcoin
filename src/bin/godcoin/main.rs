extern crate tokio_signal;
extern crate sodiumoxide;
extern crate num_traits;
extern crate env_logger;
extern crate godcoin;
extern crate tokio;
extern crate dirs;
extern crate clap;

#[macro_use]
extern crate log;

use clap::{Arg, App, AppSettings, SubCommand};
use tokio::prelude::*;
use std::sync::Arc;
use godcoin::*;

struct StartNode<'a> {
    bind_address: Option<&'a str>,
    minter_key: Option<KeyPair>
}

fn generate_keypair() {
    let pair = KeyPair::gen_keypair();
    info!("~~ Keys have been generated ~~");
    info!("Private key WIF: {}", pair.1.to_wif());
    info!("Public key WIF: {}", pair.0.to_wif());
    info!("- Make sure the keys are securely stored");
    info!("- Coins cannot be recovered if you lose your private key");
    info!("- Never give private keys to anyone");
}

fn start_node(node_opts: StartNode) {
    use godcoin::blockchain::*;
    use std::{env, path::*};

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
            info!("Created GODcoin home at {:?}", &home);
        } else {
            info!("Found GODcoin home at {:?}", &home);
        }
        home
    }.canonicalize().unwrap();

    let mut blockchain = Blockchain::new(&home);
    {
        let create_genesis = {
            let lock = blockchain.store.lock().unwrap();
            let store = lock.borrow();
            store.get(0).is_none()
        };
        if create_genesis && node_opts.minter_key.is_some() {
            if let Some(ref key) = node_opts.minter_key {
                blockchain.create_genesis_block(key);
            }
        }
    }

    info!("Using height in block log at {}", blockchain.indexer.get_chain_height());

    if let Some(ref key) = node_opts.minter_key {
        let bond = blockchain.indexer.get_bond(&key.0).expect("No bond found for minter key");
        let minter = key.clone();
        let staker = bond.staker;
        let producer = producer::Producer::new(Arc::new(blockchain), minter, staker);
        producer.start_timer();
    }

    if let Some(bind) = node_opts.bind_address {
        let addr = bind.parse()
                        .expect(&format!("Failed to parse address: {:?}", bind));
        net::start_server(&addr);
    }
}

fn main() {
    let env = env_logger::Env::new().filter_or(env_logger::DEFAULT_FILTER_ENV, "godcoin=info");
    env_logger::init_from_env(env);

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

    let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.spawn(future::lazy(move || {
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
        error!("Startup failure: {:?}", err);
    }));

    let stream = tokio_signal::ctrl_c().flatten_stream().map_err(move |e| {
        error!("Failed to handle ctrl-c event {:?}, forcing signal event", e);
    });
    stream.into_future().wait().ok().unwrap();

    println!("Received ctrl-c signal, shutting down...");
    rt.shutdown_now().wait().ok().unwrap();
}
