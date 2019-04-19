use godcoin::{
    net::*,
    prelude::{KeyPair, PrivateKey, Wif},
};
use reqwest::{Client, Url};
use rustyline::{error::ReadlineError, Editor};
use std::{
    io::{Cursor, Read},
    path::PathBuf,
};

mod db;
mod parser;

use self::db::{Db, DbState, Password};

macro_rules! check_unlocked {
    ($self:expr) => {
        if $self.db.state() != DbState::Unlocked {
            return Err("wallet not unlocked".to_owned());
        }
    };
}

macro_rules! check_args {
    ($args:expr, $count:expr) => {
        if $args.len() != $count {
            return Err("Missing arguments or too many provided".to_owned());
        }
    };
}

macro_rules! send_print_rpc_req {
    ($wallet:expr, $req:expr) => {
        let res = Client::new()
            .post($wallet.url.clone())
            .body($req.serialize())
            .send();
        match res {
            Ok(mut res) => {
                let len = res.content_length().unwrap_or(0);
                let mut content = Vec::with_capacity(len as usize);
                res.read_to_end(&mut content)
                    .map_err(|e| format!("{}", e))?;
                let mut cursor = Cursor::<&[u8]>::new(&content);
                let res = MsgResponse::deserialize(&mut cursor)
                    .map_err(|e| format!("Failed to deserialize response: {}", e))?;
                println!("{:#?}", res);
            }
            Err(e) => return Err(format!("{}", e)),
        }
    };
}

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
                    println!("Closing walllet...");
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
            "new" => {
                let state = self.db.state();
                if state != DbState::New {
                    if state == DbState::Locked {
                        println!("Use unlock to use the existing wallet");
                        return Ok(false);
                    } else if state == DbState::Unlocked {
                        println!("Existing wallet already unlocked");
                        return Ok(false);
                    } else {
                        return Err(format!("Unknown state: {:?}", state));
                    }
                }

                check_args!(args, 2);
                let pass = &Password(args.remove(1).into_bytes());
                self.db.set_password(pass);
                self.prompt = "locked>> ".to_owned();
                return Ok(false);
            }
            "unlock" => {
                let state = self.db.state();
                if state != DbState::Locked {
                    if state == DbState::New {
                        println!("A wallet has not yet been created, use new to create one");
                        return Ok(false);
                    } else if state == DbState::Unlocked {
                        println!("Wallet already unlocked");
                        return Ok(false);
                    }
                    return Err(format!("Unknown state: {:?}", state));
                }

                check_args!(args, 2);
                let pass = &Password(args.remove(1).into_bytes());
                if self.db.unlock(pass) {
                    self.prompt = "unlocked>> ".to_owned();
                } else {
                    println!("Failed to unlock wallet...incorrect password");
                }
                return Ok(false);
            }
            "create_account" => {
                check_unlocked!(self);
                let account = &args[1];
                if self.db.get_account(account).is_some() {
                    println!("Account already exists");
                    return Ok(true);
                }
                let key = KeyPair::gen_keypair();
                self.db.set_account(account, &key.1);
                println!("Public key => {}", key.0.to_wif());
                println!("Private key => {}", key.1.to_wif());
            }
            "import_account" => {
                check_unlocked!(self);
                check_args!(args, 3);
                let account = &args[1];
                let wif = PrivateKey::from_wif(&args[2]).map_err(|_| "Invalid wif".to_owned())?;
                for (acc, pair) in self.db.get_accounts() {
                    if &acc == account {
                        println!("Account already exists");
                        return Ok(true);
                    } else if pair.1 == wif.1 {
                        println!("Wif already exists under account `{}`", &acc);
                        return Ok(true);
                    }
                }
                self.db.set_account(account, &wif.1);
            }
            "get_account" => {
                check_unlocked!(self);
                check_args!(args, 2);
                let key = self.db.get_account(&args[1]);
                match key {
                    Some(key) => {
                        println!("Public key => {}", key.0.to_wif());
                        println!("Private key => {}", key.1.to_wif());
                    }
                    None => {
                        println!("Account not found");
                    }
                }
            }
            "delete_account" => {
                check_unlocked!(self);
                check_args!(args, 2);
                if self.db.del_account(&args[1]) {
                    println!("Account permanently deleted");
                } else {
                    println!("Account not found");
                }
            }
            "list_accounts" => {
                check_unlocked!(self);
                println!("Accounts:");
                for (acc, key) in self.db.get_accounts() {
                    println!("  {} => {}", acc, key.0.to_wif());
                }
            }
            "get_properties" => {
                send_print_rpc_req!(self, MsgRequest::GetProperties);
            }
            "get_block" => {
                check_args!(args, 2);
                let height: u64 = args[1]
                    .parse()
                    .map_err(|_| "Failed to parse height argument".to_owned())?;

                send_print_rpc_req!(self, MsgRequest::GetBlock(height));
            }
            "help" => {
                Self::print_usage("Displaying help...");
            }
            _ => {
                Self::print_usage(&format!("Invalid command: {}", args[0]));
            }
        }
        Ok(true)
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
        cmds.push(["list_accounts", "List all accounts"]);

        let mut max_len = 0;
        for cmd in &cmds {
            assert!(cmd.len() == 2);
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
