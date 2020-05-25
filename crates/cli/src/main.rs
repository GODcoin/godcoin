use clap::{App, AppSettings, Arg, SubCommand};
use std::{
    env,
    path::{Path, PathBuf},
};

mod keypair;
mod wallet;

use self::keypair::*;
use self::wallet::*;

fn main() {
    godcoin::init().unwrap();
    let app = App::new("godcoin")
        .about("GODcoin core CLI")
        .version(env!("CARGO_PKG_VERSION"))
        .setting(AppSettings::VersionlessSubcommands)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(SubCommand::with_name("keygen").about("Generates a keypair"))
        .subcommand(
            SubCommand::with_name("wallet")
                .about("Opens the GODcoin CLI wallet")
                .arg(
                    Arg::with_name("node_url")
                        .long("node-url")
                        .default_value("ws://localhost:7777")
                        .empty_values(false)
                        .help("Connects to the following node"),
                ),
        );
    let matches = app.get_matches();

    if matches.subcommand_matches("keygen").is_some() {
        generate_keypair();
    } else if let Some(matches) = matches.subcommand_matches("wallet") {
        let home: PathBuf = {
            let home = {
                match env::var("GODCOIN_HOME") {
                    Ok(s) => PathBuf::from(s),
                    Err(_) => Path::join(&dirs::data_local_dir().unwrap(), "godcoin"),
                }
            };
            if !Path::is_dir(&home) {
                let res = std::fs::create_dir(&home);
                res.unwrap_or_else(|_| panic!("Failed to create dir at {:?}", &home));
                println!("Created GODcoin home at {:?}", &home);
            } else {
                println!("Found GODcoin home at {:?}", &home);
            }
            home
        };

        let url = matches.value_of("node_url").unwrap();
        Wallet::new(home, url).start();
    } else {
        println!("Failed to match subcommand");
        std::process::exit(1);
    }
}
