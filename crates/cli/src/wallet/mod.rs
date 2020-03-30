use clap::{App, AppSettings, Arg, SubCommand};
use rustyline::{error::ReadlineError, Editor};
use std::path::PathBuf;
use url::Url;

mod cmd;
mod db;
mod parser;
mod script_builder;

use self::db::{Db, DbState};

pub struct Wallet {
    prompt: String,
    url: Url,
    db: Db,
    // Current ID to be sent when making requests
    req_id: u32,
}

impl Wallet {
    pub fn new(home: PathBuf, url: &str) -> Wallet {
        let db = Db::new(home.join("wallet_db"));
        let prompt = (if db.state() == DbState::Locked {
            "locked>> "
        } else {
            "new>> "
        })
        .to_owned();

        let mut url: Url = url.parse().unwrap();
        if url.host_str().is_none() {
            panic!("Expected url to have host");
        }
        if url.port().is_none() {
            url.set_port(Some(7777)).unwrap();
        }
        match url.scheme() {
            "ws" | "wss" => {}
            _ => panic!("Expected node URL scheme to be ws or wss"),
        }

        Wallet {
            db,
            prompt,
            url,
            req_id: 0,
        }
    }

    pub fn start(mut self) {
        let mut rl = Editor::<()>::new();
        loop {
            let readline = rl.readline(&self.prompt);
            match readline {
                Ok(line) => {
                    if line.is_empty() {
                        continue;
                    }
                    let args = parser::parse_line(&line);
                    let (store_history, err_msg) = self.process_line(&args);
                    if store_history {
                        rl.add_history_entry(line);
                    } else {
                        sodiumoxide::utils::memzero(&mut line.into_bytes());
                    }

                    for a in args {
                        sodiumoxide::utils::memzero(&mut a.into_bytes());
                    }

                    if let Err(msg) = err_msg {
                        println!("{}", msg);
                    }
                }
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                    println!("Closing wallet...");
                    break;
                }
                Err(err) => {
                    println!("Error reading input: {:?}", err);
                    break;
                }
            }
        }
    }

    fn process_line(&mut self, args: &[String]) -> (bool, Result<(), String>) {
        if args.is_empty() {
            return (false, Ok(()));
        }
        println!();

        let cli = App::new("")
            .setting(AppSettings::NoBinaryName)
            .setting(AppSettings::DisableVersion)
            .setting(AppSettings::VersionlessSubcommands)
            .subcommand(
                SubCommand::with_name("new")
                    .about("Create a new wallet")
                    .arg(
                        Arg::with_name("password")
                            .required(true)
                            .help("Password used to encrypt the wallet"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("unlock")
                    .about("Unlocks an existing wallet")
                    .arg(
                        Arg::with_name("password")
                            .required(true)
                            .help("Password used to unlock the wallet"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("gen_keypair")
                    .about("Generates a keypair for use with accounts")
            )
            .subcommand(
                SubCommand::with_name("account_id_to_address")
                    .about("Converts an ID to a pay-to-address WIF")
                    .arg(
                        Arg::with_name("id")
                            .required(true)
                            .takes_value(true)
                            .help("Decimal ID to convert")
                    )
            )
            .subcommand(
                SubCommand::with_name("import_account")
                    .about("Import an account")
                    .arg(
                        Arg::with_name("name")
                            .long("name")
                            .required(true)
                            .takes_value(true)
                            .help("The name of the account (stored in the wallet only)"),
                    )
                    .arg(
                        Arg::with_name("account")
                            .long("account")
                            .required(true)
                            .takes_value(true)
                            .help("The account address"),
                    )
                    .arg(
                        Arg::with_name("wif")
                            .long("wif")
                            .required(true)
                            .takes_value(true)
                            .multiple(true)
                            .help(
                                "Private WIF key for the account [accepts multiple for multisig]",
                            ),
                    ),
            )
            .subcommand(
                SubCommand::with_name("delete_account")
                    .about("Delete an account")
                    .arg(
                        Arg::with_name("name")
                            .long("name")
                            .required(true)
                            .takes_value(true)
                            .help("Name of the account to delete"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("list_accounts")
                    .about("List available accounts in the wallet"),
            )
            .subcommand(
                SubCommand::with_name("get_account")
                    .about("Retrieve account keys and addresses")
                    .arg(
                        Arg::with_name("name")
                            .long("name")
                            .required(true)
                            .takes_value(true)
                            .help("Name of the account"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("get_account_info")
                    .about("Retrieve address information from the blockchain")
                    .arg(
                        Arg::with_name("account")
                            .long("account")
                            .required(true)
                            .takes_value(true)
                            .help("Wallet account name or ID"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("build_script")
                    .about("Builds a script with the provided ops")
                    .arg(
                        Arg::with_name("ops")
                            .required(true)
                            .takes_value(true)
                            .multiple(true)
                            .help("Script operations"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("args_to_bin")
                    .about("Convert an array of arguments to hex to pass as script args")
                    .arg(
                        Arg::with_name("arg")
                            .required(true)
                            .takes_value(true)
                            .multiple(true)
                            .help("An argument that starts address={} or asset={}")
                    )
            )
            .subcommand(
                SubCommand::with_name("check_script_size")
                    .about("Checks if the script is too large and prints the size in bytes")
                    .arg(
                        Arg::with_name("hex")
                            .required(true)
                            .takes_value(true)
                            .help("Binary script in hex format"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("decode_tx")
                    .about("Decodes a transaction and prints it to console")
                    .arg(
                        Arg::with_name("hex")
                            .required(true)
                            .takes_value(true)
                            .help("Binary transaction in hex format"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("sign_tx")
                    .about("Signs a raw transaction")
                    .arg(
                        Arg::with_name("hex")
                            .long("hex")
                            .required(true)
                            .takes_value(true)
                            .help("Binary transaction in hex format"),
                    )
                    .arg(
                        Arg::with_name("account")
                            .long("account")
                            .required(true)
                            .takes_value(true)
                            .multiple(true)
                            .help("Account to sign the transaction, accepts multiple"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("unsign_tx")
                    .about("Removes a signature from a raw transaction")
                    .arg(
                        Arg::with_name("hex")
                            .long("hex")
                            .required(true)
                            .takes_value(true)
                            .help("Binary transaction in hex format"),
                    )
                    .arg(
                        Arg::with_name("index")
                            .long("index")
                            .required(true)
                            .takes_value(true)
                            .help("Index position of the signature to remove"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("broadcast")
                    .about("Broadcast a transaction to the network")
                    .arg(
                        Arg::with_name("hex")
                            .required(true)
                            .takes_value(true)
                            .help("Binary transaction in hex format"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("build_create_account_tx")
                    .about("Builds a create account transaction")
                    .arg(
                        Arg::with_name("expiry")
                            .long("expiry")
                            .takes_value(true)
                            .required(true)
                            .default_value("60000")
                            .help("The time in milliseconds when a transaction expires from now"),
                    )
                    .arg(
                        Arg::with_name("fee")
                            .long("fee")
                            .takes_value(true)
                            .required(true)
                            .help("The fee to pay for the transaction"),
                    )
                    .arg(
                        Arg::with_name("creator")
                            .long("creator")
                            .takes_value(true)
                            .required(true)
                            .help("The creator account or account address"),
                    )
                    .arg(
                        Arg::with_name("balance")
                            .long("balance")
                            .takes_value(true)
                            .required(true)
                            .help("The balance to be deposited in the new account"),
                    )
                    .arg(
                        Arg::with_name("script")
                            .long("script")
                            .takes_value(true)
                            .required(false)
                            .help(
                                "The script to apply to the account, otherwise use a default script"
                            ),
                    )
                    .arg(
                        Arg::with_name("threshold")
                            .long("threshold")
                            .takes_value(true)
                            .required(true)
                            .help("Account permissions signatory threshold for verification"),
                    )
                    .arg(
                        Arg::with_name("public_wif")
                            .long("public-wif")
                            .takes_value(true)
                            .required(true)
                            .multiple(true)
                            .help("Public key allowed for signing to meet the signatory threshold"),
                    )
            )
            .subcommand(
                SubCommand::with_name("build_mint_tx")
                    .about("Builds a mint transaction")
                    .arg(
                        Arg::with_name("expiry")
                            .long("expiry")
                            .takes_value(true)
                            .required(true)
                            .default_value("60000")
                            .help("The time in milliseconds when a transaction expires from now"),
                    )
                    .arg(
                        Arg::with_name("amount")
                            .long("amount")
                            .takes_value(true)
                            .required(true)
                            .help("The amount of tokens to be minted"),
                    )
                    .arg(
                        Arg::with_name("attachment_path")
                            .long("attachment-path")
                            .takes_value(true)
                            .requires("attachment_name")
                            .help("The path to the attachment for the transaction"),
                    )
                    .arg(
                        Arg::with_name("attachment_name")
                            .long("attachment-name")
                            .takes_value(true)
                            .help("The name of the attachment"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("build_transfer_tx")
                    .about("Builds a transfer transaction")
                    .arg(
                        Arg::with_name("expiry")
                            .long("expiry")
                            .takes_value(true)
                            .required(true)
                            .default_value("60000")
                            .help("The time in milliseconds when a transaction expires from now"),
                    )
                    .arg(
                        Arg::with_name("from_account")
                            .long("from-account")
                            .takes_value(true)
                            .required(true)
                            .help("The account to transfer from"),
                    )
                    .arg(
                        Arg::with_name("call_fn")
                            .long("call-fn")
                            .takes_value(true)
                            .required(true)
                            .help("The function to call in the script"),
                    )
                    .arg(
                        Arg::with_name("args")
                            .long("args")
                            .takes_value(true)
                            .help("The hex value of the arguments that the script requires"),
                    )
                    .arg(
                        Arg::with_name("amount")
                            .long("amount")
                            .takes_value(true)
                            .required(true)
                            .help("The amount of tokens allowed to be transferred"),
                    )
                    .arg(
                        Arg::with_name("fee")
                            .long("fee")
                            .takes_value(true)
                            .required(true)
                            .help("The fee to pay for the transaction"),
                    )
                    .arg(
                        Arg::with_name("memo")
                            .long("memo")
                            .takes_value(true)
                            .help("The memo to send with the transaction"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("get_properties").about("Retrieve network properties"),
            )
            .subcommand(
                SubCommand::with_name("get_block")
                    .about("Retrieve a block from the network")
                    .arg(
                        Arg::with_name("height")
                            .required(true)
                            .takes_value(true)
                            .help("The height of the block to retrieve"),
                    ),
            )
            .get_matches_from_safe(args);

        match cli {
            Ok(args) => match args.subcommand() {
                ("new", Some(args)) => (false, cmd::create_wallet(self, args)),
                ("unlock", Some(args)) => (false, cmd::unlock(self, args)),
                ("gen_keypair", Some(_)) => (true, Ok(crate::generate_keypair())),
                ("account_id_to_address", Some(args)) => {
                    (true, cmd::account::account_id_to_address(self, args))
                }
                ("import_account", Some(args)) => (true, cmd::account::import(self, args)),
                ("delete_account", Some(args)) => (true, cmd::account::delete(self, args)),
                ("list_accounts", Some(args)) => (true, cmd::account::list(self, args)),
                ("get_account", Some(args)) => (true, cmd::account::get(self, args)),
                ("get_account_info", Some(args)) => (true, cmd::account::get_acc_info(self, args)),
                ("build_script", Some(args)) => (true, cmd::build_script(self, args)),
                ("args_to_bin", Some(args)) => (true, cmd::args_to_bin(self, args)),
                ("check_script_size", Some(args)) => (true, cmd::check_script_size(self, args)),
                ("decode_tx", Some(args)) => (true, cmd::decode_tx(self, args)),
                ("sign_tx", Some(args)) => (true, cmd::sign_tx(self, args)),
                ("unsign_tx", Some(args)) => (true, cmd::unsign_tx(self, args)),
                ("broadcast", Some(args)) => (true, cmd::broadcast(self, args)),
                ("build_create_account_tx", Some(args)) => {
                    (true, cmd::account::build_create_account_tx(self, args))
                }
                ("build_mint_tx", Some(args)) => (true, cmd::build_mint_tx(self, args)),
                ("build_transfer_tx", Some(args)) => (true, cmd::build_transfer_tx(self, args)),
                ("get_properties", Some(args)) => (true, cmd::get_properties(self, args)),
                ("get_block", Some(args)) => (true, cmd::get_block(self, args)),
                _ => panic!("No subcommands matched: {:#?}", args),
            },
            Err(e) => (true, Err(format!("{}", e.message))),
        }
    }
}
