use actix::prelude::*;
use actix_web::{http, middleware, server, App, HttpRequest, HttpResponse};
use env_logger::{Env, DEFAULT_FILTER_ENV};
use godcoin::prelude::*;
use log::info;
use std::{
    env,
    path::{Path, PathBuf},
};

mod jsonrpc;
mod method;
mod minter;

use jsonrpc::*;
use minter::Minter;

fn index(body: String) -> HttpResponse {
    let value = serde_json::from_str::<serde_json::Value>(&body);
    match value {
        Ok(value) => match value {
            serde_json::Value::Object(_) => {
                let val = method::process_req_value(value);
                if let Some(val) = val {
                    let json =
                        serde_json::to_string(&val).expect("failed to convert value to string");
                    HttpResponse::Ok().body(json).into()
                } else {
                    HttpResponse::Ok().into()
                }
            }
            serde_json::Value::Array(array) => {
                let mut vec = Vec::with_capacity(array.len());
                for value in array {
                    let val = method::process_req_value(value);
                    if let Some(val) = val {
                        vec.push(val);
                    }
                }

                let json = serde_json::to_string(&vec).expect("failed to convert value to string");
                HttpResponse::Ok().body(json).into()
            }
            _ => {
                let msg = "expected object or array".to_owned();
                let info = ErrorInfo::new(ErrCode::InvalidReq, msg);
                ErrResponse::new(None, info).into()
            }
        },
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
