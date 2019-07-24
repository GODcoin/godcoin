use clap::{App, Arg};
use env_logger::{Env, DEFAULT_FILTER_ENV};
use godcoin::{blockchain::ReindexOpts, prelude::*};
use log::info;
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use tokio::{prelude::*, runtime::Runtime};

#[derive(Debug, Deserialize)]
struct Config {
    minter_key: String,
    enable_stale_production: bool,
    bind_address: Option<String>,
}

fn main() {
    env_logger::init_from_env(Env::new().filter_or(DEFAULT_FILTER_ENV, "godcoin=info"));
    godcoin::init().unwrap();

    let home = {
        match env::var("GODCOIN_HOME") {
            Ok(s) => PathBuf::from(s),
            Err(_) => Path::join(&dirs::data_local_dir().unwrap(), "godcoin"),
        }
    };

    let home = home.to_string_lossy();
    let args = App::new("godcoin-server")
        .about("GODcoin core server daemon")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::with_name("home")
                .long("home")
                .default_value(&home)
                .empty_values(false)
                .help("Home directory which defaults to env var GODCOIN_HOME"),
        )
        .arg(
            Arg::with_name("reindex")
                .long("reindex")
                .help("Reindexes the block log"),
        )
        .arg(
            Arg::with_name("auto_trim")
                .long("reindex-trim-corrupt")
                .help("Trims any corruption detected in the block log during reindexing"),
        )
        .get_matches();

    let home = PathBuf::from(args.value_of("home").expect("Failed to obtain home path"));
    let (blocklog_loc, index_loc) = {
        if !Path::is_dir(&home) {
            let res = std::fs::create_dir(&home);
            res.unwrap_or_else(|_| panic!("Failed to create dir at {:?}", &home));
            info!("Created GODcoin home at {:?}", &home);
        } else {
            info!("Found GODcoin home at {:?}", &home);
        }
        let blocklog_loc = Path::join(&home, "blklog");
        let index_loc = Path::join(&home, "index");
        (blocklog_loc, index_loc)
    };

    let config_file = Path::join(&home, "config.toml");
    info!("Opening configuration file at {:?}", config_file);
    let config_file = fs::read(config_file).expect("Failed to open config");
    let config: Config = toml::from_str(&String::from_utf8(config_file).unwrap()).unwrap();

    let minter_key =
        PrivateKey::from_wif(&config.minter_key).expect("Provided minter key is invalid");
    let bind_addr = config
        .bind_address
        .unwrap_or_else(|| "127.0.0.1:7777".to_owned());

    let reindex = if args.is_present("reindex") {
        info!("User requested reindexing");
        if Path::exists(&index_loc) {
            info!("Deleting current index");
            fs::remove_dir_all(&index_loc)
                .expect("Failed to delete the blockchain index directory");
        } else {
            info!("Current index does not exist");
        }
        let auto_trim = args.is_present("auto_trim");
        Some(ReindexOpts { auto_trim })
    } else {
        None
    };

    let mut rt = Runtime::new().unwrap();

    let enable_stale_production = config.enable_stale_production;
    rt.spawn(future::lazy(move || {
        godcoin_server::start(godcoin_server::ServerOpts {
            blocklog_loc,
            index_loc,
            minter_key,
            bind_addr,
            reindex,
            enable_stale_production,
        });
        Ok(())
    }));

    let future = tokio_signal::ctrl_c()
        .flatten_stream()
        .into_future()
        .and_then(|_| {
            info!("Received ctrl-c, shutting down...");
            Ok(())
        });
    rt.block_on(future).map_err(|(e, _)| e).unwrap();
}
