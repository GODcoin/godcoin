use reqwest::Url;
use rustyline::{error::ReadlineError, Editor};
use std::path::PathBuf;

mod cmd;
mod db;
mod parser;
mod script_builder;

use self::db::{Db, DbState};

pub struct Wallet {
    prompt: String,
    url: Url,
    db: Db,
}

impl Wallet {
    pub fn new(home: PathBuf) -> Wallet {
        let db = Db::new(home.join("db"));
        let prompt = (if db.state() == DbState::Locked {
            "locked>> "
        } else {
            "new>> "
        })
        .to_owned();
        Wallet {
            db,
            prompt,
            url: "http://localhost:7777".parse().unwrap(),
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
                    let mut args = parser::parse_line(&line);

                    match self.process_line(&mut args) {
                        Ok(store_history) => {
                            if store_history {
                                rl.add_history_entry(line);
                            } else {
                                sodiumoxide::utils::memzero(&mut line.into_bytes());
                            }
                        }
                        Err(s) => {
                            println!("{}", s);
                            sodiumoxide::utils::memzero(&mut line.into_bytes());
                        }
                    }

                    for a in args {
                        sodiumoxide::utils::memzero(&mut a.into_bytes());
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

    fn process_line(&mut self, args: &mut Vec<String>) -> Result<bool, String> {
        if args.is_empty() {
            return Ok(false);
        }
        match &*args[0] {
            "new" => cmd::create_wallet(self, args),
            "unlock" => cmd::unlock(self, args),
            "create_account" => cmd::account::create(self, args),
            "import_account" => cmd::account::import(self, args),
            "get_account" => cmd::account::get(self, args),
            "delete_account" => cmd::account::delete(self, args),
            "list_accounts" => cmd::account::list(self, args),
            "build_script" => cmd::build_script(self, args),
            "decode_tx" => cmd::decode_tx(self, args),
            "build_mint_tx" => cmd::build_mint_tx(self, args),
            "get_properties" => cmd::get_properties(self, args),
            "get_block" => cmd::get_block(self, args),
            "help" => {
                Self::print_usage("Displaying help...");
                Ok(true)
            }
            _ => {
                Self::print_usage(&format!("Invalid command: {}", args[0]));
                Ok(true)
            }
        }
    }

    fn print_usage(header: &str) {
        let mut cmds = Vec::<[&str; 2]>::new();
        cmds.push(["help", "Display this help menu"]);
        cmds.push(["new <password>", "Create a new wallet"]);
        cmds.push(["unlock <password>", "Unlock an existing wallet"]);
        cmds.push(["create_account <account>", "Create an account"]);
        cmds.push(["import_account <account> <wif>", "Import an account"]);
        cmds.push(["delete_account <account>", "Delete an existing account"]);
        cmds.push(["get_account <account>", "Retrieve account information"]);
        cmds.push(["list_accounts", "List all accounts"]);
        cmds.push(["build_script <...op>", "Builds a script"]);
        cmds.push([
            "decode_tx <tx_hex>",
            "Decodes a transaction and prints it to console",
        ]);
        cmds.push([
            "build_mint_tx <timestamp_offset> <gold_asset> <silver_asset> <owner_script>",
            "Builds a mint transaction",
        ]);
        cmds.push(["get_properties", "Retrieve global network properties"]);
        cmds.push(["get_block <height>", "Retrieve a block from the network"]);

        let mut max_len = 0;
        for cmd in &cmds {
            assert_eq!(cmd.len(), 2);
            let cmd_len = cmd[0].len();
            if cmd_len > max_len {
                max_len = cmd_len;
            }
        }

        println!("{}\n", header);
        for cmd in &cmds {
            let mut c = cmd[0].to_owned();
            if c.len() < max_len {
                for _ in 0..max_len - c.len() {
                    c.push(' ');
                }
            }
            println!("  {}  {}", c, cmd[1]);
        }
        println!();
    }
}
