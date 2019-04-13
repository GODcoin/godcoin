use actix::prelude::*;
use actix_web::{http, middleware, server, App, HttpRequest, HttpResponse};
use env_logger::{Env, DEFAULT_FILTER_ENV};
use godcoin::prelude::*;
use log::info;
use std::{
    env,
    path::{Path, PathBuf},
};

mod minter;
mod jsonrpc;
mod method;

use minter::Minter;
use jsonrpc::*;

fn index(body: String) -> HttpResponse {
    let request = serde_json::from_str::<jsonrpc::Request>(&body);
    match request {
        Ok(request) => method::process_req(request),
        Err(e) => {
            use serde_json::error::Category;
            let code = match e.classify() {
                Category::Syntax | Category::Eof => ErrCode::ParseError,
                Category::Data => ErrCode::InvalidReq,
                _ => ErrCode::InternalError,
            };
            let info = ErrorInfo::new(code, e.to_string());
            ErrResponse::new(None, info).into()
        }
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

    let blockchain = Blockchain::new(&home);
    info!(
        "Using height in block log at {}",
        blockchain.get_chain_height()
    );

    if blockchain.get_block(0).is_none() {
        blockchain.create_genesis_block(&minter_key);
    }

    let wallet_addr = blockchain.get_owner().wallet;
    Minter::new(blockchain, minter_key, wallet_addr).start();

    server::HttpServer::new(|| {
        App::new()
            .middleware(middleware::Logger::new(r#"%a "%r" %s %T"#))
            .resource("/", |r| {
                r.method(http::Method::POST).with_config(index, |cfg| {
                    // Limit 64 KiB
                    cfg.0.limit(65536);
                })
            })
            .default_resource(|r| {
                r.with(|_: HttpRequest| HttpResponse::NotFound().body("Not found"))
            })
    })
    .bind(env::var("GODCOIN_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_owned()))
    .unwrap()
    .start();

    sys.run();
}
