use futures::sync::mpsc::{self, UnboundedSender};
use godcoin::{blockchain::ReindexOpts, net::*, prelude::*};
use log::{error, info, warn};
use std::{io::Cursor, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, prelude::*};
use tokio_tungstenite::tungstenite::{protocol, Message};

mod forever;
pub mod minter;
pub mod pool;

pub mod prelude {
    pub use super::minter::*;
    pub use super::pool::SubscriptionPool;
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
        tokio::spawn(
            tokio_tungstenite::accept_async(stream)
                .and_then(move |ws| {
                    info!("[{}] Connection opened", peer_addr);

                    let (tx, rx) = mpsc::unbounded();
                    let (sink, stream) = ws.split();
                    let mut state = WsState::new(peer_addr, tx.clone());

                    let ws_reader = stream.for_each({
                        let data = Arc::clone(&data);
                        move |msg| {
                            let res = process_message(&data, &mut state, msg);
                            if let Some(res) = res {
                                tx.unbounded_send(res).unwrap();
                            }
                            Ok(())
                        }
                    });
                    let ws_writer = rx.forward(sink.sink_map_err(move |e| {
                        error!("[{}] Sink send error: {:?}", peer_addr, e);
                    }));

                    let conn = ws_reader
                        .map_err(|_| ())
                        .select(ws_writer.map(|_| ()).map_err(|_| ()));
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

pub fn process_message(data: &ServerData, state: &mut WsState, msg: Message) -> Option<Message> {
    match msg {
        Message::Binary(buf) => {
            let mut cur = Cursor::<&[u8]>::new(&buf);
            let res = match Request::deserialize(&mut cur) {
                Ok(req) => {
                    let id = req.id;
                    if id == u32::max_value() {
                        // Max value is reserved
                        Response {
                            id: u32::max_value(),
                            body: ResponseBody::Error(ErrorKind::Io),
                        }
                    } else if cur.position() != buf.len() as u64 {
                        Response {
                            id,
                            body: ResponseBody::Error(ErrorKind::BytesRemaining),
                        }
                    } else {
                        Response {
                            id,
                            body: handle_request(data, state, req.body),
                        }
                    }
                }
                Err(e) => {
                    error!("Error occurred during deserialization: {:?}", e);
                    Response {
                        id: u32::max_value(),
                        body: ResponseBody::Error(ErrorKind::Io),
                    }
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

fn handle_request(data: &ServerData, state: &mut WsState, body: RequestBody) -> ResponseBody {
    match body {
        RequestBody::Broadcast(tx) => {
            let res = data.minter.push_tx(tx);
            match res {
                Ok(_) => ResponseBody::Broadcast,
                Err(e) => ResponseBody::Error(ErrorKind::TxValidation(e)),
            }
        }
        RequestBody::SetBlockFilter(filter) => {
            if filter.len() > 16 {
                return ResponseBody::Error(ErrorKind::InvalidRequest);
            }
            state.filter = Some(filter);
            ResponseBody::SetBlockFilter
        }
        RequestBody::ClearBlockFilter => {
            state.filter = None;
            ResponseBody::ClearBlockFilter
        }
        RequestBody::Subscribe => {
            data.sub_pool.insert(state.addr(), state.sender());
            ResponseBody::Subscribe
        }
        RequestBody::Unsubscribe => {
            data.sub_pool.remove(state.addr());
            ResponseBody::Unsubscribe
        }
        RequestBody::GetProperties => {
            let props = data.chain.get_properties();
            ResponseBody::GetProperties(props)
        }
        RequestBody::GetBlock(height) => match &state.filter {
            Some(filter) => match data.chain.get_filtered_block(height, filter) {
                Some(block) => ResponseBody::GetBlock(block),
                None => ResponseBody::Error(ErrorKind::InvalidHeight),
            },
            None => match data.chain.get_block(height) {
                Some(block) => ResponseBody::GetBlock(FilteredBlock::Block(block)),
                None => ResponseBody::Error(ErrorKind::InvalidHeight),
            },
        },
        RequestBody::GetFullBlock(height) => match data.chain.get_block(height) {
            Some(block) => ResponseBody::GetFullBlock(block),
            None => ResponseBody::Error(ErrorKind::InvalidHeight),
        },
        RequestBody::GetAddressInfo(addr) => {
            let res = data.minter.get_addr_info(&addr);
            match res {
                Ok(info) => ResponseBody::GetAddressInfo(info),
                Err(e) => ResponseBody::Error(ErrorKind::TxValidation(e)),
            }
        }
    }
}

pub struct WsState {
    filter: Option<BlockFilter>,
    addr: SocketAddr,
    tx: UnboundedSender<Message>,
}

impl WsState {
    #[inline]
    pub fn new(addr: SocketAddr, tx: UnboundedSender<Message>) -> Self {
        Self {
            filter: None,
            addr,
            tx,
        }
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
    pub fn sender(&self) -> UnboundedSender<Message> {
        self.tx.clone()
    }
}
