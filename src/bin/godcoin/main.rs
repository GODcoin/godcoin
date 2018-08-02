extern crate num_traits;
extern crate godcoin;
extern crate clap;

use clap::{Arg, App, AppSettings, SubCommand};

struct StartNode<'a> {
    bind_address: Option<&'a str>,
    port: Option<u16>
}

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

fn start_node(node_opts: StartNode) {
    use godcoin::net::*;

    if let Some(bind) = node_opts.bind_address {
        let port = node_opts.port.unwrap();

        let addr = format!("{}:{}", bind, port).parse().unwrap();
        start_server(&addr);
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
                                .help("Bind address endpoint - Incoming P2P connections are disabled if not provided")
                                .long("bind")
                                .value_name("address"))
                            .arg(Arg::with_name("port")
                                .help("Endpoint bind port")
                                .long("port")
                                .value_name("port")
                                .default_value("7777")));
    let matches = app.get_matches();

    if let Some(_) = matches.subcommand_matches("keygen") {
        generate_keypair();
    } else if let Some(matches) = matches.subcommand_matches("node") {
        start_node(StartNode {
            bind_address: matches.value_of("bind_address"),
            port: matches.value_of("port").unwrap().parse::<u16>().ok()
        });
    }
}
