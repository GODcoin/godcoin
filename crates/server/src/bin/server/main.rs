use env_logger::{Env, DEFAULT_FILTER_ENV};
use godcoin::prelude::*;
use log::info;
use std::{
    env,
    path::{Path, PathBuf},
};

fn main() {
    env_logger::init_from_env(Env::new().filter_or(DEFAULT_FILTER_ENV, "godcoin=info,actix=info"));
    godcoin::init().unwrap();
    let sys = actix::System::new("godcoin-server");

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
            info!("Created GODcoin home at {:?}", &home);
        } else {
            info!("Found GODcoin home at {:?}", &home);
        }
        home
    };

    let minter_key = {
        let key =
            env::var("GODCOIN_MINTER_KEY").expect("GODCOIN_MINTER_KEY is required to init server");
        PrivateKey::from_wif(&key).expect("GODCOIN_MINTER_KEY is invalid")
    };

    let bind_addr = env::var("GODCOIN_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:7777".to_owned());

    godcoin_server::start(godcoin_server::ServerOpts {
        home,
        minter_key,
        bind_addr,
    });

    sys.run().unwrap();
}
