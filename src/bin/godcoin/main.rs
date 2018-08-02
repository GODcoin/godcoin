extern crate num_traits;
extern crate godcoin;
extern crate tokio;
extern crate clap;

use clap::{Arg, App, AppSettings, SubCommand};
use tokio::prelude::*;

struct StartNode<'a> {
    bind_address: Option<&'a str>,
}

fn generate_keypair() {
    use godcoin::crypto::*;

    let pair = KeyPair::gen_keypair();
    println!("~~ Keys have been generated ~~");
    println!("Private key WIF: {}", pair.1.to_wif());
    println!("Public key WIF: {}", pair.0.to_wif());
    println!("- Make sure the keys are securely stored");
    println!("- Coins cannot be recovered if you lose your private key");
    println!("- Never give private keys to anyone");
}

fn start_node(node_opts: StartNode) {
    use godcoin::net::*;

    if let Some(bind) = node_opts.bind_address {
        let addr = bind.parse();
        match addr {
            Ok(ref addr) => start_server(addr),
            Err(e) => panic!("Failed to parse address: {:?}", e)
        }
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
                                .value_name("address")));
    let matches = app.get_matches();

    ::tokio::run(future::lazy(move || {
        use ::std::io::{Error, ErrorKind};

        if let Some(_) = matches.subcommand_matches("keygen") {
            generate_keypair();
        } else if let Some(matches) = matches.subcommand_matches("node") {
            start_node(StartNode {
                bind_address: matches.value_of("bind_address")
            });
        } else {
            return Err(Error::new(ErrorKind::Other, "Failed to match subcommand"))
        }

        Ok(())
    }).map_err(|err| {
        println!("Startup failure: {:?}", err);
    }));
}
