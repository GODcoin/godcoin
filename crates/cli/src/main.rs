use clap::{App, AppSettings, Arg, SubCommand};
use godcoin::*;
use log::error;
use std::sync::mpsc;
use tokio::prelude::*;

mod keypair;
mod node;

use self::keypair::*;
use self::node::*;

fn main() {
    let env = env_logger::Env::new().filter_or(env_logger::DEFAULT_FILTER_ENV, "godcoin=info");
    env_logger::init_from_env(env);

    godcoin::init().unwrap();
    let app = App::new("godcoin")
        .about("GODcoin core CLI")
        .version(env!("CARGO_PKG_VERSION"))
        .setting(AppSettings::VersionlessSubcommands)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(SubCommand::with_name("keygen").about("Generates a keypair"))
        .subcommand(
            SubCommand::with_name("node")
                .about("Starts the blockchain node service")
                .arg(
                    Arg::with_name("bind_address")
                        .help("Bind address endpoint (i.e 0.0.0.0:7777)")
                        .env("GODCOIN_BIND_ADDRESS")
                        .long("bind")
                        .value_name("address"),
                )
                .arg(
                    Arg::with_name("peers")
                        .help("Comma-separated list of peers")
                        .env("GODCOIN_PEERS")
                        .long("peers")
                        .value_delimiter(",")
                        .value_name("peers")
                        .use_delimiter(true),
                )
                .arg(
                    Arg::with_name("minter_key")
                        .help("Private minting key required to mint")
                        .env("GODCOIN_MINTER_KEY")
                        .long("minter-key")
                        .value_name("key"),
                ),
        );
    let matches = app.get_matches();

    let (tx, rx) = mpsc::channel::<()>();
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    {
        let tx = tx.clone();
        rt.block_on(
            future::lazy(move || {
                use ::std::io::{Error, ErrorKind};

                if matches.subcommand_matches("keygen").is_some() {
                    generate_keypair(&tx);
                } else if let Some(matches) = matches.subcommand_matches("node") {
                    let node = Node {
                        bind_address: matches.value_of("bind_address"),
                        peers: matches.values_of("peers").map(|p| p.collect()),
                        minter_key: matches.value_of("minter_key").map(|s| {
                            godcoin::PrivateKey::from_wif(s)
                                .expect("Failed to parse minter key argument")
                        }),
                    };
                    node.start();
                } else {
                    return Err(Error::new(ErrorKind::Other, "Failed to match subcommand"));
                }

                Ok(())
            })
            .map_err(|err| {
                error!("Startup failure: {:?}", err);
            }),
        )
        .unwrap();
    }

    ctrlc::set_handler(move || {
        println!("Received ctrl-c signal, shutting down...");
        tx.send(()).unwrap();
    })
    .unwrap();

    rx.recv().unwrap();
    rt.shutdown_now().wait().ok().unwrap();
}
