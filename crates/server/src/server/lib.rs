use actix_web::{middleware, web, App, HttpResponse, HttpServer};
use godcoin::{blockchain::ReindexOpts, net::*, prelude::*};
use log::{error, info, warn};
use std::{io::Cursor, path::PathBuf, sync::Arc};

pub mod minter;
pub mod net;

pub mod prelude {
    pub use super::minter::*;
    pub use super::net::*;
}

use prelude::*;

pub struct ServerOpts {
    pub blocklog_loc: PathBuf,
    pub index_loc: PathBuf,
    pub minter_key: KeyPair,
    pub bind_addr: String,
    pub reindex: Option<ReindexOpts>,
}

#[derive(Clone)]
pub struct ServerData {
    pub chain: Arc<Blockchain>,
    pub minter: Minter,
}

pub fn start(opts: ServerOpts) {
    let blockchain = Arc::new(Blockchain::new(&opts.blocklog_loc, &opts.index_loc));

    let is_empty = blockchain.is_empty();
    if !is_empty && blockchain.index_status() != IndexStatus::Complete {
        warn!(
            "Indexing not complete (status = {:?})",
            blockchain.index_status()
        );
        match opts.reindex {
            Some(opts) => blockchain.reindex(opts),
            None => panic!("index incomplete, aborting..."),
        }
    }

    if is_empty {
        let info = blockchain.create_genesis_block(opts.minter_key.clone());
        info!("=> Generated new block chain");
        info!("=> {:?}", info.script);
        for (index, key) in info.wallet_keys.iter().enumerate() {
            info!("=> Wallet key {}: {}", index + 1, key.1.to_wif());
        }
    }

    info!(
        "Using height in block log at {}",
        blockchain.get_chain_height()
    );

    let minter = Minter::new(Arc::clone(&blockchain), opts.minter_key);
    minter.clone().start_production_loop();

    HttpServer::new(move || {
        App::new()
            .data(ServerData {
                chain: Arc::clone(&blockchain),
                minter: minter.clone(),
            })
            .wrap(middleware::Logger::new(r#"%a "%r" %s %T"#))
            .service(
                web::resource("/")
                    .data({
                        // Limit 64 KiB
                        web::PayloadConfig::default().limit(65536)
                    })
                    .route(web::post().to(index)),
            )
    })
    .bind(opts.bind_addr)
    .unwrap()
    .start();
}

pub fn index(data: web::Data<ServerData>, body: bytes::Bytes) -> HttpResponse {
    let mut cur = Cursor::<&[u8]>::new(&body);
    match RequestType::deserialize(&mut cur) {
        Ok(req_type) => {
            if cur.position() != body.len() as u64 {
                return ResponseType::Single(MsgResponse::Error(ErrorKind::BytesRemaining))
                    .into_res();
            }
            handle_request_type(&data, req_type).into_res()
        }
        Err(e) => {
            error!("Unknown error occurred during deserialization: {:?}", e);
            ResponseType::Single(MsgResponse::Error(ErrorKind::Io)).into_res()
        }
    }
}

fn handle_request_type(data: &ServerData, req_type: RequestType) -> ResponseType {
    match req_type {
        RequestType::Batch(mut reqs) => {
            let mut responses = Vec::with_capacity(reqs.len());
            for req in reqs.drain(..) {
                responses.push(handle_direct_request(&data, req));
            }

            ResponseType::Batch(responses)
        }
        RequestType::Single(req) => ResponseType::Single(handle_direct_request(&data, req)),
    }
}

fn handle_direct_request(data: &ServerData, req: MsgRequest) -> MsgResponse {
    match req {
        MsgRequest::Broadcast(tx) => {
            let res = data.minter.push_tx(tx);
            match res {
                Ok(_) => MsgResponse::Broadcast,
                Err(e) => MsgResponse::Error(ErrorKind::TxValidation(e)),
            }
        }
        MsgRequest::GetProperties => {
            let props = data.chain.get_properties();
            MsgResponse::GetProperties(props)
        }
        MsgRequest::GetBlock(height) => match data.chain.get_block(height) {
            Some(block) => MsgResponse::GetBlock(block.as_ref().clone()),
            None => MsgResponse::Error(ErrorKind::InvalidHeight),
        },
        MsgRequest::GetAddressInfo(addr) => {
            let res = data.minter.get_addr_info(&addr);
            match res {
                Ok(info) => MsgResponse::GetAddressInfo(info),
                Err(e) => MsgResponse::Error(ErrorKind::TxValidation(e)),
            }
        }
    }
}
