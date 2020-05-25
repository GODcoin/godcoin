mod block_range;

use crate::{metrics::*, ServerData};
use block_range::AsyncBlockRange;
use futures::{
    channel::mpsc::{self, Sender},
    prelude::*,
};
use godcoin::{get_epoch_time, net::*, prelude::*};
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
use tracing::{debug, error, info, warn};
use tracing_futures::Instrument;

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

    let client_fut = async move {
        let ws_stream = match tokio_tungstenite::accept_async_with_config(stream, config).await {
            Ok(ws) => ws,
            Err(e) => {
                error!("WebSocket accept error: {:?}", e);
                return;
            }
        };
        info!("Connection opened");

        let (tx, rx) = mpsc::channel(32);
        let (sink, mut stream) = ws_stream.split();
        let mut state = WsClient::new(peer_addr, tx.clone());
        let needs_pong = state.needs_pong();

        let ws_reader = {
            let data = Arc::clone(&data);
            let mut tx = tx.clone();
            async move {
                while let Some(msg) = stream.next().await {
                    match msg {
                        Ok(msg) => {
                            let res = process_ws_msg(&data, &mut state, msg);
                            if let Some(res) = res {
                                if let Err(e) = tx.send(res).await {
                                    warn!("Failed to send message: {:?}", e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Error reading WS message: {:?}", e);
                            break;
                        }
                    }
                }
            }
        };

        let ws_writer = rx
            .inspect(|item| match item {
                WsMessage::Binary(bytes) => NET_BYTES_SENT.inc_by(bytes.len() as i64),
                _ => {}
            })
            .map(|msg| Ok(msg))
            .forward(sink.sink_map_err(move |e| {
                warn!("Sink send error: {:?}", e);
            }));

        let heartbeat_interval = async move {
            let dur = Duration::from_secs(20);
            let mut interval = time::interval_at(time::Instant::now() + dur, dur);
            loop {
                interval.tick().await;
                if needs_pong.swap(true, Ordering::AcqRel) {
                    debug!("Did not receive pong in time from peer");
                    break;
                }

                let nonce = get_epoch_time();
                let msg = Msg {
                    id: u32::max_value(),
                    body: Body::Ping(nonce),
                };
                debug!("Sending ping: {}", nonce);

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

        info!("Connection closed");
        // Remove block subscriptions if there are any
        data.sub_pool.remove(peer_addr);
    };

    let span = tracing::info_span!("client_connection", peer_addr = ?peer_addr);
    tokio::spawn(client_fut.instrument(span));
}

pub fn process_ws_msg(
    data: &ServerData,
    state: &mut WsClient,
    msg: WsMessage,
) -> Option<WsMessage> {
    match msg {
        WsMessage::Binary(buf) => {
            NET_BYTES_RECEIVED.inc_by(buf.len() as i64);
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

fn handle_protocol_msg(data: &ServerData, state: &mut WsClient, msg: Msg) -> Option<Body> {
    match msg.body {
        Body::Error(e) => {
            warn!("Received error message from client: {:?}", e);
            None
        }
        Body::Request(req) => handle_rpc_request(data, state, msg.id, req),
        Body::Response(res) => {
            warn!("Unexpected response from client: {:?}", res);
            None
        }
        Body::Ping(nonce) => {
            debug!("Received ping: {}", nonce);
            Some(Body::Pong(nonce))
        }
        Body::Pong(nonce) => {
            debug!("Received pong: {}", nonce);
            // We don't need to update the `needs_pong` state as it has already been updated when
            // the message was deserialized
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
            REQ_BROADCAST_TOTAL.inc();
            let req_timer = REQ_BROADCAST_DUR.start_timer();
            let res = data.minter.push_tx(tx);
            req_timer.stop_and_record();
            match res {
                Ok(_) => Body::Response(rpc::Response::Broadcast),
                Err(e) => {
                    REQ_BROADCAST_FAIL.inc();
                    Body::Error(ErrorKind::TxValidation(e))
                }
            }
        }
        rpc::Request::SetBlockFilter(filter) => {
            let req_timer = REQ_SET_BLOCK_FILTER_DUR.start_timer();
            if filter.len() > 16 {
                return Some(Body::Error(ErrorKind::InvalidRequest));
            }
            state.filter = Some(filter);
            req_timer.stop_and_record();
            Body::Response(rpc::Response::SetBlockFilter)
        }
        rpc::Request::ClearBlockFilter => {
            let req_timer = REQ_CLEAR_BLOCK_FILTER_DUR.start_timer();
            state.filter = None;
            req_timer.stop_and_record();
            Body::Response(rpc::Response::ClearBlockFilter)
        }
        rpc::Request::Subscribe => {
            let req_timer = REQ_SUBSCRIBE_DUR.start_timer();
            data.sub_pool.insert(state.addr(), state.sender());
            req_timer.stop_and_record();
            Body::Response(rpc::Response::Subscribe)
        }
        rpc::Request::Unsubscribe => {
            let req_timer = REQ_UNSUBSCRIBE_DUR.start_timer();
            data.sub_pool.remove(state.addr());
            req_timer.stop_and_record();
            Body::Response(rpc::Response::Unsubscribe)
        }
        rpc::Request::GetProperties => {
            let req_timer = REQ_GET_PROPERTIES_DUR.start_timer();
            let props = data.chain.get_properties();
            req_timer.stop_and_record();
            Body::Response(rpc::Response::GetProperties(props))
        }
        rpc::Request::GetBlock(height) => {
            let req_timer = REQ_GET_BLOCK_DUR.start_timer();
            let res = match &state.filter {
                Some(filter) => match data.chain.get_filtered_block(height, filter) {
                    Some(block) => Body::Response(rpc::Response::GetBlock(block)),
                    None => Body::Error(ErrorKind::InvalidHeight),
                },
                None => match data.chain.get_block(height) {
                    Some(block) => {
                        Body::Response(rpc::Response::GetBlock(FilteredBlock::Block(block)))
                    }
                    None => Body::Error(ErrorKind::InvalidHeight),
                },
            };
            req_timer.stop_and_record();
            res
        }
        rpc::Request::GetFullBlock(height) => {
            let req_timer = REQ_GET_FULL_BLOCK_DUR.start_timer();
            let res = match data.chain.get_block(height) {
                Some(block) => Body::Response(rpc::Response::GetFullBlock(block)),
                None => Body::Error(ErrorKind::InvalidHeight),
            };
            req_timer.stop_and_record();
            res
        }
        rpc::Request::GetBlockRange(min_height, max_height) => {
            let req_timer = REQ_GET_BLOCK_RANGE_DUR.start_timer();
            let range = AsyncBlockRange::try_new(Arc::clone(&data.chain), min_height, max_height);
            match range {
                Some(mut range) => {
                    if let Some(filter) = state.filter() {
                        range.set_filter(Some(filter.clone()));
                    }

                    {
                        let mut tx = state.sender();
                        let fut = async move {
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
                                    warn!("Failed to send block range update");
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
                                warn!("Failed to send block range finalizer");
                            }
                        };
                        tokio::spawn(fut.in_current_span());
                    }

                    req_timer.stop_and_record();
                    return None;
                }
                None => {
                    req_timer.stop_and_record();
                    Body::Error(ErrorKind::InvalidHeight)
                }
            }
        }
        rpc::Request::GetAccountInfo(acc) => {
            let req_timer = REQ_GET_ACC_INFO_DUR.start_timer();
            let res = data.minter.get_account_info(acc);
            req_timer.stop_and_record();
            match res {
                Ok(info) => Body::Response(rpc::Response::GetAccountInfo(info)),
                Err(e) => Body::Error(ErrorKind::TxValidation(e)),
            }
        }
    })
}
