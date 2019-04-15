use clap::{App, AppSettings, SubCommand};
use std::{path::{PathBuf, Path}, env};

mod keypair;
mod wallet;

use self::keypair::*;
use self::wallet::*;

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
        .subcommand(SubCommand::with_name("wallet").about("Opens the GODcoin CLI wallet"));
    let matches = app.get_matches();

    if matches.subcommand_matches("keygen").is_some() {
        generate_keypair();
    } else if matches.subcommand_matches("wallet").is_some() {
        let home: PathBuf = {
            let home = {
                use dirs;
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

        Wallet::new(home).start();
    } else {
        println!("Failed to match subcommand");
        std::process::exit(1);
    }
}
