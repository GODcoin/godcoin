use futures::sync::mpsc::{self, Sender};
use godcoin::{blockchain::ReindexOpts, get_epoch_ms, net::*, prelude::*};
use log::{debug, error, info, warn};
use std::{
    io::Cursor,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{net::TcpListener, prelude::*, timer::Interval};
use tokio_tungstenite::tungstenite::{protocol, Message as WsMessage};

mod block_range;
mod forever;
pub mod minter;
pub mod pool;

pub mod prelude {
    pub use super::minter::*;
    pub use super::pool::SubscriptionPool;
}

use block_range::AsyncBlockRange;
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
    pub sub_pool: SubscriptionPool,
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

    let sub_pool = SubscriptionPool::new();
    let minter = Minter::new(
        Arc::clone(&blockchain),
        opts.minter_key,
        sub_pool.clone(),
        opts.enable_stale_production,
    );
    minter.clone().start_production_loop();

    let data = Arc::new(ServerData {
        chain: Arc::clone(&blockchain),
        minter,
        sub_pool,
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
        let config = Some(protocol::WebSocketConfig {
            // # of protocol Message's
            max_send_queue: Some(16),
            // 64 MiB
            max_message_size: Some(64 << 20),
            // 16 MiB
            max_frame_size: Some(16 << 20),
        });
        tokio::spawn(
            tokio_tungstenite::accept_async_with_config(stream, config)
                .and_then(move |ws| {
                    info!("[{}] Connection opened", peer_addr);

                    let (tx, rx) = mpsc::channel(32);
                    let (sink, stream) = ws.split();
                    let mut state = WsState::new(peer_addr, tx.clone());
                    let needs_pong = state.needs_pong();

                    let ws_reader = stream.for_each({
                        let data = Arc::clone(&data);
                        let tx = tx.clone();
                        move |msg| {
                            let res = process_ws_message(&data, &mut state, msg);
                            if let Some(res) = res {
                                future::Either::A(tx.clone().send(res).then(move |res| {
                                    if res.is_err() {
                                        error!("[{}] Failed to send message", peer_addr);
                                    }
                                    Ok(())
                                }))
                            } else {
                                future::Either::B(future::ok(()))
                            }
                        }
                    });
                    let ws_writer = rx.forward(sink.sink_map_err(move |e| {
                        error!("[{}] Sink send error: {:?}", peer_addr, e);
                    }));

                    let heartbeat_interval = Interval::new_interval(Duration::from_secs(20))
                        .take_while(move |_| Ok(!needs_pong.swap(true, Ordering::AcqRel)))
                        .for_each(move |_| {
                            let nonce = get_epoch_ms();
                            let msg = Msg {
                                id: u32::max_value(),
                                body: Body::Ping(nonce),
                            };
                            debug!("[{}] Sending ping: {}", peer_addr, nonce);

                            let mut buf = Vec::with_capacity(16);
                            msg.serialize(&mut buf);
                            tx.clone().send(WsMessage::Binary(buf)).then(|_| Ok(()))
                        });

                    let conn = ws_reader.select2(ws_writer).select2(heartbeat_interval);
                    tokio::spawn(conn.then(move |_| {
                        info!("[{}] Connection closed", peer_addr);
                        // Remove block subscriptions if there are any
                        data.sub_pool.remove(peer_addr);
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

pub fn process_ws_message(
    data: &ServerData,
    state: &mut WsState,
    msg: WsMessage,
) -> Option<WsMessage> {
    match msg {
        WsMessage::Binary(buf) => {
            state.set_needs_pong(false);

            let mut cur = Cursor::<&[u8]>::new(&buf);
            let msg = match Msg::deserialize(&mut cur) {
                Ok(msg) => {
                    let id = msg.id;
                    if cur.position() != buf.len() as u64 {
                        Msg {
                            id,
                            body: Body::Error(ErrorKind::BytesRemaining),
                        }
                    } else {
                        match handle_protocol_message(data, state, msg) {
                            Some(body) => Msg { id, body },
                            None => return None,
                        }
                    }
                }
                Err(e) => {
                    error!("Error occurred during deserialization: {:?}", e);
                    Msg {
                        id: u32::max_value(),
                        body: Body::Error(ErrorKind::Io),
                    }
                }
            };

            let mut buf = Vec::with_capacity(65536);
            msg.serialize(&mut buf);
            Some(WsMessage::Binary(buf))
        }
        WsMessage::Text(_) => Some(WsMessage::Close(Some(protocol::CloseFrame {
            code: protocol::frame::coding::CloseCode::Unsupported,
            reason: "text is not supported".into(),
        }))),
        _ => None,
    }
}

fn handle_protocol_message(data: &ServerData, state: &mut WsState, msg: Msg) -> Option<Body> {
    match msg.body {
        Body::Error(e) => {
            warn!(
                "[{}] Received error message from client: {:?}",
                state.addr(),
                e
            );
            None
        }
        Body::Request(req) => handle_rpc_request(data, state, msg.id, req),
        Body::Response(res) => {
            warn!(
                "[{}] Unexpected response from client: {:?}",
                state.addr(),
                res
            );
            None
        }
        Body::Ping(nonce) => {
            debug!("[{}] Received ping: {}", state.addr(), nonce);
            Some(Body::Pong(nonce))
        }
        Body::Pong(nonce) => {
            debug!("[{}] Received pong: {}", state.addr(), nonce);
            // We don't need to update the `needs_pong` state as it has already been updated when the message was
            // deserialized
            None
        }
    }
}

fn handle_rpc_request(
    data: &ServerData,
    state: &mut WsState,
    id: u32,
    req: rpc::Request,
) -> Option<Body> {
    Some(match req {
        rpc::Request::Broadcast(tx) => {
            let res = data.minter.push_tx(tx);
            match res {
                Ok(_) => Body::Response(rpc::Response::Broadcast),
                Err(e) => Body::Error(ErrorKind::TxValidation(e)),
            }
        }
        rpc::Request::SetBlockFilter(filter) => {
            if filter.len() > 16 {
                return Some(Body::Error(ErrorKind::InvalidRequest));
            }
            state.filter = Some(filter);
            Body::Response(rpc::Response::SetBlockFilter)
        }
        rpc::Request::ClearBlockFilter => {
            state.filter = None;
            Body::Response(rpc::Response::ClearBlockFilter)
        }
        rpc::Request::Subscribe => {
            data.sub_pool.insert(state.addr(), state.sender());
            Body::Response(rpc::Response::Subscribe)
        }
        rpc::Request::Unsubscribe => {
            data.sub_pool.remove(state.addr());
            Body::Response(rpc::Response::Unsubscribe)
        }
        rpc::Request::GetProperties => {
            let props = data.chain.get_properties();
            Body::Response(rpc::Response::GetProperties(props))
        }
        rpc::Request::GetBlock(height) => match &state.filter {
            Some(filter) => match data.chain.get_filtered_block(height, filter) {
                Some(block) => Body::Response(rpc::Response::GetBlock(block)),
                None => Body::Error(ErrorKind::InvalidHeight),
            },
            None => match data.chain.get_block(height) {
                Some(block) => Body::Response(rpc::Response::GetBlock(FilteredBlock::Block(block))),
                None => Body::Error(ErrorKind::InvalidHeight),
            },
        },
        rpc::Request::GetFullBlock(height) => match data.chain.get_block(height) {
            Some(block) => Body::Response(rpc::Response::GetFullBlock(block)),
            None => Body::Error(ErrorKind::InvalidHeight),
        },
        rpc::Request::GetBlockRange(min_height, max_height) => {
            let range = AsyncBlockRange::try_new(Arc::clone(&data.chain), min_height, max_height);
            match range {
                Some(mut range) => {
                    if let Some(filter) = state.filter() {
                        range.set_filter(Some(filter.clone()));
                    }

                    let peer_addr = state.addr();
                    let tx = state.sender();
                    tokio::spawn(
                        range
                            .map(move |block| {
                                let msg = Msg {
                                    id,
                                    body: Body::Response(rpc::Response::GetBlock(block)),
                                };

                                let mut buf = Vec::with_capacity(65536);
                                msg.serialize(&mut buf);
                                WsMessage::Binary(buf)
                            })
                            .forward(tx.clone().sink_map_err(move |_| {
                                error!("[{}] Failed to send block range update", peer_addr);
                            }))
                            .and_then(move |_| {
                                let msg = Msg {
                                    id,
                                    body: Body::Response(rpc::Response::GetBlockRange),
                                };

                                let mut buf = Vec::with_capacity(32);
                                msg.serialize(&mut buf);
                                tx.send(WsMessage::Binary(buf))
                                    .map(|_sink| ())
                                    .map_err(move |_| {
                                        error!(
                                            "[{}] Failed to send block range finalizer",
                                            peer_addr
                                        );
                                    })
                            }),
                    );

                    return None;
                }
                None => Body::Error(ErrorKind::InvalidHeight),
            }
        }
        rpc::Request::GetAddressInfo(addr) => {
            let res = data.minter.get_addr_info(&addr);
            match res {
                Ok(info) => Body::Response(rpc::Response::GetAddressInfo(info)),
                Err(e) => Body::Error(ErrorKind::TxValidation(e)),
            }
        }
    })
}

pub struct WsState {
    filter: Option<BlockFilter>,
    addr: SocketAddr,
    tx: Sender<WsMessage>,
    needs_pong: Arc<AtomicBool>,
}

impl WsState {
    #[inline]
    pub fn new(addr: SocketAddr, tx: Sender<WsMessage>) -> Self {
        Self {
            filter: None,
            addr,
            tx,
            needs_pong: Arc::new(AtomicBool::new(false)),
        }
    }

    #[inline]
    pub fn needs_pong(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.needs_pong)
    }

    #[inline]
    pub fn set_needs_pong(&self, flag: bool) {
        self.needs_pong.store(flag, Ordering::Release);
    }

    #[inline]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    #[inline]
    pub fn filter(&self) -> Option<&BlockFilter> {
        self.filter.as_ref()
    }

    #[inline]
    pub fn sender(&self) -> Sender<WsMessage> {
        self.tx.clone()
    }
}
