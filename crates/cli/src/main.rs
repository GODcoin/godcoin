use clap::{App, AppSettings, SubCommand};
use log::error;
use std::sync::mpsc;
use tokio::prelude::*;

mod keypair;

use self::keypair::*;

fn main() {
    let env = env_logger::Env::new().filter_or(env_logger::DEFAULT_FILTER_ENV, "godcoin=info");
    env_logger::init_from_env(env);

    godcoin::init().unwrap();
    let app = App::new("godcoin")
        .about("GODcoin core CLI")
        .version(env!("CARGO_PKG_VERSION"))
        .setting(AppSettings::VersionlessSubcommands)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(SubCommand::with_name("keygen").about("Generates a keypair"));
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
