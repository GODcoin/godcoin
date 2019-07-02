use clap::{App, Arg};
use env_logger::{Env, DEFAULT_FILTER_ENV};
use godcoin::{blockchain::ReindexOpts, prelude::*};
use log::info;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    env_logger::init_from_env(Env::new().filter_or(DEFAULT_FILTER_ENV, "godcoin=info,actix=info"));
    godcoin::init().unwrap();

    let args = App::new("godcoin-server")
        .about("GODcoin core server daemon")
        .version(env!("CARGO_PKG_VERSION"))
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

    let (blocklog_loc, index_loc) = {
        let home = {
            match env::var("GODCOIN_HOME") {
                Ok(s) => PathBuf::from(s),
                Err(_) => Path::join(&dirs::data_local_dir().unwrap(), "godcoin"),
            }
        };
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

    let minter_key = {
        let key =
            env::var("GODCOIN_MINTER_KEY").expect("GODCOIN_MINTER_KEY is required to init server");
        PrivateKey::from_wif(&key).expect("GODCOIN_MINTER_KEY is invalid")
    };

    let bind_addr = env::var("GODCOIN_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:7777".to_owned());

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

    let sys = actix::System::new("godcoin-server");
    godcoin_server::start(godcoin_server::ServerOpts {
        blocklog_loc,
        index_loc,
        minter_key,
        bind_addr,
        reindex,
    });
    sys.run().unwrap();
}
