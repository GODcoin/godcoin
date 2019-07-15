use actix::prelude::*;
use actix_web::{middleware, web, App, HttpResponse, HttpServer};
use futures::{
    future::{join_all, ok},
    Future,
};
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
    pub minter: Addr<Minter>,
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

    let minter = Minter::new(Arc::clone(&blockchain), opts.minter_key).start();
    minter.do_send(minter::StartProductionLoop);

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
                    .route(web::post().to_async(index)),
            )
    })
    .bind(opts.bind_addr)
    .unwrap()
    .start();
}

pub fn index(
    data: web::Data<ServerData>,
    body: bytes::Bytes,
) -> Box<Future<Item = HttpResponse, Error = ()>> {
    let mut cur = Cursor::<&[u8]>::new(&body);
    match RequestType::deserialize(&mut cur) {
        Ok(req_type) => {
            if cur.position() != body.len() as u64 {
                return Box::new(ok(ResponseType::Single(MsgResponse::Error(
                    ErrorKind::BytesRemaining,
                ))
                .into_res()));
            }
            Box::new(handle_request_type(&data, req_type).map(IntoHttpResponse::into_res))
        }
        Err(e) => {
            error!("Unknown error occurred during deserialization: {:?}", e);
            Box::new(ok(
                ResponseType::Single(MsgResponse::Error(ErrorKind::Io)).into_res()
            ))
        }
    }
}

fn handle_request_type(
    data: &ServerData,
    req_type: RequestType,
) -> Box<Future<Item = ResponseType, Error = ()> + Send> {
    match req_type {
        RequestType::Batch(mut reqs) => {
            let mut futs = Vec::with_capacity(reqs.len());
            for req in reqs.drain(..) {
                futs.push(handle_direct_request(&data, req));
            }

            Box::new(join_all(futs).map(|responses| ResponseType::Batch(responses)))
        }
        RequestType::Single(req) => {
            Box::new(handle_direct_request(&data, req).map(ResponseType::Single))
        }
    }
}

fn handle_direct_request(
    data: &ServerData,
    req: MsgRequest,
) -> Box<Future<Item = MsgResponse, Error = ()> + Send> {
    match req {
        MsgRequest::Broadcast(tx) => {
            let fut = data.minter.send(minter::PushTx(tx)).then(|res| {
                Ok(match res.unwrap() {
                    Ok(_) => MsgResponse::Broadcast,
                    Err(e) => MsgResponse::Error(ErrorKind::TxValidation(e)),
                })
            });
            Box::new(fut)
        }
        MsgRequest::GetProperties => {
            let props = data.chain.get_properties();
            Box::new(ok(MsgResponse::GetProperties(props)))
        }
        MsgRequest::GetBlock(height) => match data.chain.get_block(height) {
            Some(block) => Box::new(ok(MsgResponse::GetBlock(block.as_ref().clone()))),
            None => Box::new(ok(MsgResponse::Error(ErrorKind::InvalidHeight))),
        },
        MsgRequest::GetAddressInfo(addr) => {
            let fut = data.minter.send(minter::GetAddrInfo(addr)).then(|res| {
                Ok(match res.unwrap() {
                    Ok(info) => MsgResponse::GetAddressInfo(info),
                    Err(e) => MsgResponse::Error(ErrorKind::TxValidation(e)),
                })
            });
            Box::new(fut)
        }
    }
}
