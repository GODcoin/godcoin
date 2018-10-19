extern crate env_logger;
extern crate godcoin;
extern crate tokio;
extern crate ctrlc;
extern crate dirs;
extern crate clap;

#[macro_use]
extern crate log;

use godcoin::{*, net::PeerPool, producer::Producer};
use clap::{Arg, App, AppSettings, SubCommand};
use std::sync::{Arc, mpsc};
use tokio::prelude::*;

struct StartNode<'a> {
    bind_address: Option<&'a str>,
    minter_key: Option<KeyPair>,
    peers: Option<Vec<&'a str>>
}

fn generate_keypair(shutdown_handle: &mpsc::Sender<()>) {
    let pair = KeyPair::gen_keypair();
    info!("~~ Keys have been generated ~~");
    info!("Private key WIF: {}", pair.1.to_wif());
    info!("Public key WIF: {}", pair.0.to_wif());
    info!("- Make sure the keys are securely stored");
    info!("- Coins cannot be recovered if you lose your private key");
    info!("- Never give private keys to anyone");
    shutdown_handle.send(()).unwrap();
}

fn start_node(node_opts: &StartNode) {
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
            res.unwrap_or_else(|_| panic!("Failed to create dir at {:?}", &home));
            info!("Created GODcoin home at {:?}", &home);
        } else {
            info!("Found GODcoin home at {:?}", &home);
        }
        home
    };

    let mut blockchain = Blockchain::new(&home);
    {
        let create_genesis = blockchain.get_block(0).is_none();
        if create_genesis && node_opts.minter_key.is_some() {
            if let Some(ref key) = node_opts.minter_key {
                blockchain.create_genesis_block(key);
            }
        }
    }

    info!("Using height in block log at {}", blockchain.get_chain_height());

    let blockchain = Arc::new(blockchain);
    let producer = match &node_opts.minter_key {
        Some(key) => {
            let bond = blockchain.get_bond(&key.0).expect("No bond found for minter key");
            let minter = key.clone();
            let staker = bond.staker;
            let producer = Producer::new(Arc::clone(&blockchain), minter, staker);
            Arc::new(Some(producer))
        },
        None => Arc::new(None)
    };

    if let Some(peers) = &node_opts.peers {
        let pool = PeerPool::new(peers);
        pool.start(&blockchain, &producer);

        // TODO synchronize blocks with peers
    }

    if let Some(producer) = producer.as_ref() {
        producer.clone().start_timer();
    }

    if let Some(bind) = node_opts.bind_address {
        let addr = bind.parse()
                        .unwrap_or_else(|_| panic!("Failed to parse address: {:?}", bind));
        net::server::start(addr, blockchain, producer);
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
                                .env("GODCOIN_BIND_ADDRESS")
                                .long("bind")
                                .value_name("address"))
                            .arg(Arg::with_name("peers")
                                .help("Comma-separated list of peers")
                                .env("GODCOIN_PEERS")
                                .long("peers")
                                .value_delimiter(",")
                                .value_name("peers")
                                .use_delimiter(true))
                            .arg(Arg::with_name("minter_key")
                                .help("Private minting key required to mint")
                                .env("GODCOIN_MINTER_KEY")
                                .long("minter-key")
                                .value_name("key")));
    let matches = app.get_matches();

    let (tx, rx) = mpsc::channel::<()>();
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    {
        let tx = tx.clone();
        rt.block_on(future::lazy(move || {
            use ::std::io::{Error, ErrorKind};

            if matches.subcommand_matches("keygen").is_some() {
                generate_keypair(&tx);
            } else if let Some(matches) = matches.subcommand_matches("node") {
                start_node(&StartNode {
                    bind_address: matches.value_of("bind_address"),
                    peers: matches.values_of("peers").map(|p| { p.collect() }),
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
        })).unwrap();
    }

    ctrlc::set_handler(move || {
        println!("Received ctrl-c signal, shutting down...");
        tx.send(()).unwrap();
    }).unwrap();

    rx.recv().unwrap();
    rt.shutdown_now().wait().ok().unwrap();
}
