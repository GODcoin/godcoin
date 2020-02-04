mod block_range;

use crate::ServerData;
use block_range::AsyncBlockRange;
use futures::{
    channel::mpsc::{self, Sender},
    prelude::*,
};
use godcoin::{get_epoch_time, net::*, prelude::*};
use log::{debug, error, info, warn};
use std::{
    io::Cursor,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{net::TcpStream, time};
use tokio_tungstenite::tungstenite::{protocol, Message as WsMessage};

pub struct WsClient {
    filter: Option<BlockFilter>,
    addr: SocketAddr,
    tx: Sender<WsMessage>,
    needs_pong: Arc<AtomicBool>,
}

impl WsClient {
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

pub fn handle_new_client(stream: TcpStream, peer_addr: SocketAddr, data: Arc<ServerData>) {
    let config = Some(protocol::WebSocketConfig {
        // # of protocol Message's
        max_send_queue: Some(16),
        // 64 MiB
        max_message_size: Some(64 << 20),
        // 16 MiB
        max_frame_size: Some(16 << 20),
    });
    tokio::spawn(async move {
        let ws_stream = match tokio_tungstenite::accept_async_with_config(stream, config).await {
            Ok(ws) => ws,
            Err(e) => {
                error!("[{}] WebSocket accept error: {:?}", peer_addr, e);
                return;
            }
        };
        info!("[{}] Connection opened", peer_addr);

        let (tx, rx) = mpsc::channel(32);
        let (sink, mut stream) = ws_stream.split();
        let mut state = WsClient::new(peer_addr, tx.clone());
        let needs_pong = state.needs_pong();

        let ws_reader = {
            let data = Arc::clone(&data);
            let mut tx = tx.clone();
            async move {
                while let Some(msg) = stream.next().await {
                    let res = process_ws_msg(&data, &mut state, msg.unwrap());
                    if let Some(res) = res {
                        if let Err(e) = tx.send(res).await {
                            warn!("[{}] Failed to send message: {:?}", peer_addr, e);
                        }
                    }
                }
            }
        };

        let ws_writer = rx.map(|msg| Ok(msg)).forward(sink.sink_map_err(move |e| {
            warn!("[{}] Sink send error: {:?}", peer_addr, e);
        }));

        let heartbeat_interval = async move {
            let dur = Duration::from_secs(20);
            let mut interval = time::interval_at(time::Instant::now() + dur, dur);
            loop {
                interval.tick().await;
                if needs_pong.swap(true, Ordering::AcqRel) {
                    debug!("[{}] Did not receive pong in time from peer", peer_addr);
                    break;
                }

                let nonce = get_epoch_time();
                let msg = Msg {
                    id: u32::max_value(),
                    body: Body::Ping(nonce),
                };
                debug!("[{}] Sending ping: {}", peer_addr, nonce);

                let mut buf = Vec::with_capacity(16);
                msg.serialize(&mut buf);

                if tx.clone().send(WsMessage::Binary(buf)).await.is_err() {
                    break;
                }
            }
        };

        tokio::select! {
            _ = ws_reader => {},
            _ = ws_writer => {},
            _ = heartbeat_interval => {},
        };

        info!("[{}] Connection closed", peer_addr);
        // Remove block subscriptions if there are any
        data.sub_pool.remove(peer_addr);
    });
}

pub fn process_ws_msg(
    data: &ServerData,
    state: &mut WsClient,
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
                        match handle_protocol_msg(data, state, msg) {
                            Some(body) => Msg { id, body },
                            None => return None,
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "[{}] Error occurred during deserialization: {:?}",
                        state.addr(),
                        e
                    );
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

fn handle_protocol_msg(data: &ServerData, state: &mut WsClient, msg: Msg) -> Option<Body> {
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
    state: &mut WsClient,
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
                    let mut tx = state.sender();
                    tokio::spawn(async move {
                        while let Some(block) = range.next().await {
                            let ws_msg = {
                                let msg = Msg {
                                    id,
                                    body: Body::Response(rpc::Response::GetBlock(block)),
                                };

                                let mut buf = Vec::with_capacity(65536);
                                msg.serialize(&mut buf);
                                WsMessage::Binary(buf)
                            };
                            if tx.send(ws_msg).await.is_err() {
                                warn!("[{}] Failed to send block range update", peer_addr);
                                return;
                            }
                        }

                        let ws_msg = {
                            let msg = Msg {
                                id,
                                body: Body::Response(rpc::Response::GetBlockRange),
                            };

                            let mut buf = Vec::with_capacity(32);
                            msg.serialize(&mut buf);
                            WsMessage::Binary(buf)
                        };
                        if tx.send(ws_msg).await.is_err() {
                            warn!("[{}] Failed to send block range finalizer", peer_addr);
                        }
                    });

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
