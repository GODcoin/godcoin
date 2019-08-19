use futures::sync::mpsc;
use godcoin::{blockchain::ReindexOpts, net::*, prelude::*};
use log::{error, info, warn};
use std::{io::Cursor, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, prelude::*};
use tokio_tungstenite::tungstenite::{protocol, Message};

mod forever;
pub mod minter;

pub mod prelude {
    pub use super::minter::*;
}

use prelude::*;

pub struct ServerOpts {
    pub blocklog_loc: PathBuf,
    pub index_loc: PathBuf,
    pub minter_key: KeyPair,
    pub bind_addr: String,
    pub reindex: Option<ReindexOpts>,
    pub enable_stale_production: bool,
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

    let minter = Minter::new(
        Arc::clone(&blockchain),
        opts.minter_key,
        opts.enable_stale_production,
    );
    minter.clone().start_production_loop();

    let data = Arc::new(ServerData {
        chain: Arc::clone(&blockchain),
        minter,
    });

    let addr = opts.bind_addr.parse::<SocketAddr>().unwrap();
    start_server(addr, data);
}

fn start_server(server_addr: SocketAddr, data: Arc<ServerData>) {
    let server = TcpListener::bind(&server_addr).unwrap();
    let incoming = forever::ListenForever::new(server.incoming());
    tokio::spawn(incoming.for_each(move |stream| {
        let peer_addr = stream.peer_addr().unwrap();
        let data = Arc::clone(&data);
        tokio::spawn(
            tokio_tungstenite::accept_async(stream)
                .and_then(move |ws| {
                    info!("[{}] Connection opened", peer_addr);

                    let (tx, rx) = mpsc::unbounded();
                    let (sink, stream) = ws.split();

                    let ws_reader = stream.for_each(move |msg| {
                        let res = process_message(&data, msg);
                        if let Some(res) = res {
                            tx.unbounded_send(res).unwrap();
                        }
                        Ok(())
                    });
                    let ws_writer = rx.forward(sink.sink_map_err(move |e| {
                        error!("[{}] Sink send error: {:?}", peer_addr, e);
                    }));

                    let conn = ws_reader
                        .map_err(|_| ())
                        .select(ws_writer.map(|_| ()).map_err(|_| ()));
                    tokio::spawn(conn.then(move |_| {
                        info!("[{}] Connection closed", peer_addr);
                        Ok(())
                    }));

                    Ok(())
                })
                .map_err(move |e| {
                    error!("[{}] WS accept error = {:?}", peer_addr, e);
                }),
        );
        Ok(())
    }));
}

pub fn process_message(data: &ServerData, msg: Message) -> Option<Message> {
    match msg {
        Message::Binary(buf) => {
            let mut cur = Cursor::<&[u8]>::new(&buf);
            let res = match RequestType::deserialize(&mut cur) {
                Ok(req_type) => {
                    if cur.position() != buf.len() as u64 {
                        ResponseType::Single(MsgResponse::Error(ErrorKind::BytesRemaining))
                    } else {
                        handle_request_type(data, req_type)
                    }
                }
                Err(e) => {
                    error!("Unknown error occurred during deserialization: {:?}", e);
                    ResponseType::Single(MsgResponse::Error(ErrorKind::Io))
                }
            };

            let mut buf = Vec::with_capacity(65536);
            res.serialize(&mut buf);
            Some(Message::Binary(buf))
        }
        Message::Text(_) => Some(Message::Close(Some(protocol::CloseFrame {
            code: protocol::frame::coding::CloseCode::Unsupported,
            reason: "text is not supported".into(),
        }))),
        _ => None,
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
            Some(block) => MsgResponse::GetBlock(Box::new(block.as_ref().clone())),
            None => MsgResponse::Error(ErrorKind::InvalidHeight),
        },
        MsgRequest::GetBlockHeader(height) => match data.chain.get_block(height) {
            Some(block) => {
                let header = block.header();
                let signer = block.signer().expect("cannot get unsigned block").clone();
                MsgResponse::GetBlockHeader { header, signer }
            }
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
