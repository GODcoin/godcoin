use actix::prelude::*;
use actix_web::{middleware, web, App, HttpResponse, HttpServer};
use env_logger::{Env, DEFAULT_FILTER_ENV};
use godcoin::{net::*, prelude::*};
use log::{error, info};
use std::{
    env,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
};

mod minter;
mod net;

use minter::Minter;
use net::*;

struct AppData {
    chain: Arc<Blockchain>,
}

fn index(data: web::Data<AppData>, body: bytes::Bytes) -> HttpResponse {
    match MsgRequest::deserialize(&mut Cursor::new(&body)) {
        Ok(msg_req) => match msg_req {
            MsgRequest::GetProperties => {
                let chain = &data.chain;
                let props = chain.get_properties();
                MsgResponse::GetProperties(props).into_res()
            }
            MsgRequest::GetBlock(height) => {
                let chain = &data.chain;
                match chain.get_block(height) {
                    Some(block) => MsgResponse::GetBlock(block.as_ref().clone()).into_res(),
                    None => MsgResponse::Error(ErrorKind::InvalidHeight, None).into_res(),
                }
            }
        },
        Err(e) => match e.kind() {
            _ => {
                error!("Unknown error occurred during deserialization: {:?}", e);
                MsgResponse::Error(ErrorKind::UnknownError, None).into_res()
            }
        },
    }
}

fn main() {
    env_logger::init_from_env(Env::new().filter_or(DEFAULT_FILTER_ENV, "godcoin=info,actix=info"));
    godcoin::init().unwrap();
    let sys = actix::System::new("godcoin-server");

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

    let blockchain = Arc::new(Blockchain::new(&home));
    info!(
        "Using height in block log at {}",
        blockchain.get_chain_height()
    );

    if blockchain.get_block(0).is_none() {
        blockchain.create_genesis_block(&minter_key);
    }

    let wallet_addr = blockchain.get_owner().wallet;
    Minter::new(Arc::clone(&blockchain), minter_key, wallet_addr).start();

    HttpServer::new(move || {
        App::new()
            .data(AppData {
                chain: Arc::clone(&blockchain),
            })
            .wrap(middleware::Logger::new(r#"%a "%r" %s %T"#))
            .service(
                web::resource("/").route(
                    web::post()
                        .data({
                            // Limit 64 KiB
                            web::PayloadConfig::default().limit(65536)
                        })
                        .to(index),
                ),
            )
    })
    .bind(env::var("GODCOIN_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:7777".to_owned()))
    .unwrap()
    .start();

    sys.run().unwrap();
}
