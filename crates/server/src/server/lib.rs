use actix::prelude::*;
use actix_web::{middleware, web, App, HttpResponse, HttpServer};
use godcoin::{net::*, prelude::*};
use log::{error, info};
use std::{io::Cursor, path::PathBuf, sync::Arc};

pub mod minter;
pub mod net;

use minter::*;
use net::*;

pub struct ServerConfig {
    pub home: PathBuf,
    pub minter_key: KeyPair,
    pub bind_addr: String,
}

pub fn start(config: ServerConfig) {
    let blockchain = Arc::new(Blockchain::new(&config.home));
    info!(
        "Using height in block log at {}",
        blockchain.get_chain_height()
    );

    if blockchain.get_block(0).is_none() {
        blockchain.create_genesis_block(&config.minter_key);
    }

    let wallet_addr = blockchain.get_owner().wallet;
    Minter::new(Arc::clone(&blockchain), config.minter_key, wallet_addr).start();

    HttpServer::new(move || {
        App::new()
            .data(Arc::clone(&blockchain))
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
    .bind(config.bind_addr)
    .unwrap()
    .start();
}

fn index(chain: web::Data<Arc<Blockchain>>, body: bytes::Bytes) -> HttpResponse {
    match MsgRequest::deserialize(&mut Cursor::new(&body)) {
        Ok(msg_req) => handle_request(&chain, msg_req).into_res(),
        Err(e) => match e.kind() {
            _ => {
                error!("Unknown error occurred during deserialization: {:?}", e);
                MsgResponse::Error(ErrorKind::UnknownError, None).into_res()
            }
        },
    }
}

pub fn handle_request(chain: &Blockchain, req: MsgRequest) -> MsgResponse {
    match req {
        MsgRequest::GetProperties => {
            let props = chain.get_properties();
            MsgResponse::GetProperties(props)
        }
        MsgRequest::GetBlock(height) => match chain.get_block(height) {
            Some(block) => MsgResponse::GetBlock(block.as_ref().clone()),
            None => MsgResponse::Error(ErrorKind::InvalidHeight, None),
        },
    }
}
