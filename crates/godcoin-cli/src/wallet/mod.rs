use rustyline::{Editor, error::ReadlineError};
use std::path::PathBuf;
use log::error;

mod parser;

pub struct Wallet {
    _home: PathBuf
}

impl Wallet {
    pub fn new(home: PathBuf) -> Wallet {
        Wallet {
            _home: home
        }
    }

    pub fn start(self) {
        let mut rl = Editor::<()>::new();
        let prompt = "new>> ";
        loop {
            let readline = rl.readline(prompt);
            match readline {
                Ok(line) => {
                    if line.is_empty() { continue }
                    rl.add_history_entry(line.as_ref());
                    let args = parser::parse_line(&line);
                    self.process_line(args);
                },
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                    println!("Closing walllet...");
                    break
                },
                Err(err) => {
                    error!("Error: {:?}", err);
                    break
                }
            }
        }
    }

    fn process_line(&self, args: Vec<String>) {
        if args.len() == 0 { return }
        match &*args[0] {
            "help" => {
                Self::print_usage("Displaying help...");
            },
            _ => {
                Self::print_usage(&format!("Invalid command: {}", args[0]));
            }
        }
    }

    fn print_usage(header: &str) {
        let mut cmds = Vec::new();
        cmds.push(["help", "Displays this help menu"]);

        let mut max_len = 0;
        for cmd in &cmds {
            assert!(cmd.len() == 2);
            let cmd_len = cmd[0].len();
            if cmd_len > max_len { max_len = cmd_len; }
        }

        println!("{}\n", header);
        for cmd in &cmds {
            let mut c = cmd[0].to_owned();
            if c.len() < max_len {
                for _ in 0 .. max_len - c.len() { c.push(' '); }
            }
            println!("  {}  {}", c, cmd[1]);
        }
        println!("");
    }
}
