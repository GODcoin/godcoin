use actix::prelude::*;
use actix_web::{middleware, web, App, HttpServer};
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
                        .to(|chain: web::Data<Arc<Blockchain>>, body: bytes::Bytes| {
                            handle_request(&chain, &body).into_res()
                        }),
                ),
            )
    })
    .bind(config.bind_addr)
    .unwrap()
    .start();
}

pub fn handle_request(chain: &Blockchain, req: &[u8]) -> MsgResponse {
    match MsgRequest::deserialize(&mut Cursor::new(req)) {
        Ok(msg_req) => match msg_req {
            MsgRequest::GetProperties => {
                let props = chain.get_properties();
                MsgResponse::GetProperties(props)
            }
            MsgRequest::GetBlock(height) => match chain.get_block(height) {
                Some(block) => MsgResponse::GetBlock(block.as_ref().clone()),
                None => MsgResponse::Error(ErrorKind::InvalidHeight, None),
            },
        },
        Err(e) => match e.kind() {
            _ => {
                error!("Unknown error occurred during deserialization: {:?}", e);
                MsgResponse::Error(ErrorKind::UnknownError, None)
            }
        },
    }
}
