use std::sync::{Arc, atomic::{Ordering, AtomicUsize, AtomicBool}};
use std::net::SocketAddr;
use parking_lot::Mutex;
use tokio::prelude::*;
use log::warn;

use crate::blockchain::{Blockchain, SignedBlock};
use crate::producer::Minter;

use super::peer::PeerType;
use super::client::*;
use super::rpc::*;

pub struct PeerPool {
    peer_addresses: Vec<SocketAddr>,
    peers: Mutex<Vec<PeerState>>,
    peer_iter_count: AtomicUsize
}

impl PeerPool {
    pub fn new(addrs: &[&str]) -> PeerPool {
        let peer_addresses = addrs.into_iter().map(|s| {
            (*s).parse().map_err(|e| {
                format!("failed to parse address: {} {}", *s, e)
            }).unwrap()
        }).collect::<Vec<SocketAddr>>();
        let peers = Mutex::new(Vec::with_capacity(peer_addresses.len()));
        let peer_iter_count = AtomicUsize::new(0);

        PeerPool {
            peer_addresses,
            peers,
            peer_iter_count
        }
    }

    pub fn start(&self, blockchain: &Arc<Blockchain>, minter: &Arc<Option<Minter>>) {
        assert!(self.peers.lock().is_empty(), "peer pool already started");
        for addr in self.peer_addresses.clone() {
            let (tx, rx) = connect_loop(addr, PeerType::NODE);
            let state = PeerState {
                tx,
                rx,
                connected: Arc::new(AtomicBool::new(false)),
                id: Arc::new(AtomicUsize::new(1))
            };
            let blockchain = Arc::clone(blockchain);
            let minter = Arc::clone(minter);
            self.handle_client_peer(addr, blockchain, minter, state);
        }
    }

    pub fn get_block(&self, height: u64) -> Option<impl Future<Item = Option<SignedBlock>, Error = ()>> {
        let peer = self.get_next_peer();
        if let Some(peer) = peer {
            let payload = Box::new(RpcPayload {
                id: peer.id.fetch_add(1, Ordering::AcqRel) as u32,
                msg: Some(RpcMsg::Block(Box::new(RpcVariant::Req(height))))
            });

            let fut = peer.tx.send(ClientEvent::Message(payload)).and_then(|rx| {
                Ok(rx.unwrap())
            }).and_then(|rx| {
                rx
            }).map(|payload| {
                match payload.msg {
                    Some(msg) => {
                        match msg {
                            RpcMsg::Block(var) => match var.res() {
                                Some(block) => block,
                                None => None
                            },
                            _ => None
                        }
                    },
                    None => None
                }
            }).map_err(|_| {});
            return Some(fut)
        }
        None
    }

    fn handle_client_peer(&self,
                            addr: SocketAddr,
                            blockchain: Arc<Blockchain>,
                            minter: Arc<Option<Minter>>,
                            state: PeerState) {
        macro_rules! quick_send {
            ($state:expr, $id:expr, $msg:expr) => {
                $state.tx.send(ClientEvent::Message(Box::new(RpcPayload {
                    id: $id,
                    msg: Some($msg)
                })))
            };
            ($state:expr, $id:expr) => {
                $state.tx.send(ClientEvent::Message(Box::new(RpcPayload {
                    id: $id,
                    msg: None
                })))
            };
        }

        self.peers.lock().push(state.clone());
        tokio::spawn(state.rx.clone().for_each(move |evt| {
            match evt {
                ClientEvent::Connect => {
                    state.connected.store(true, Ordering::Release);
                },
                ClientEvent::Disconnect => {
                    state.connected.store(false, Ordering::Release);
                },
                ClientEvent::Message(rpc) => {
                    let id = rpc.id;
                    let msg = match rpc.msg {
                        Some(msg) => msg,
                        None => return Ok(())
                    };
                    match msg {
                        RpcMsg::Handshake(_) => {
                            warn!("[{}] Invalid handshake message sent from peer", addr);
                        }
                        RpcMsg::Error(_) => {},
                        RpcMsg::Event(evt) => {
                            if let Some(minter) = &*minter {
                                match *evt {
                                    RpcEvent::Block(block) => {
                                        let _ = minter.add_block(block);
                                    },
                                    RpcEvent::Tx(tx) => {
                                        let _ = minter.add_tx(tx);
                                    }
                                }
                            }
                        },
                        RpcMsg::Broadcast(tx) => {
                            if let Some(minter) = &*minter {
                                match minter.add_tx(tx) {
                                    Ok(_) => {
                                        quick_send!(state, id).wait().unwrap();
                                    },
                                    Err(s) => {
                                        quick_send!(state, id, RpcMsg::Error(s)).wait().unwrap();
                                    }
                                }
                            }
                        },
                        RpcMsg::Properties(var) => {
                            if var.req().is_some() {
                                let props = blockchain.get_properties();
                                let var = RpcVariant::Res(props);
                                quick_send!(state, id, RpcMsg::Properties(var)).wait().unwrap();
                            }
                        },
                        RpcMsg::Block(var) => {
                            if let Some(height) = var.req() {
                                let block = match blockchain.get_block(height) {
                                    Some(block) => Some((&*block).clone()),
                                    None => None
                                };
                                let var = Box::new(RpcVariant::Res(block));
                                quick_send!(state, id, RpcMsg::Block(var)).wait().unwrap();
                            }
                        },
                        RpcMsg::Balance(var) => {
                            if let Some(addr) = var.req() {
                                let bal = blockchain.get_balance(&addr);
                                let var = RpcVariant::Res(bal);
                                quick_send!(state, id, RpcMsg::Balance(var)).wait().unwrap();
                            }
                        },
                        RpcMsg::TotalFee(var) => {
                            if let Some(addr) = var.req() {
                                let fee = blockchain.get_total_fee(&addr);
                                match fee {
                                    Some(fee) => {
                                        let var = RpcVariant::Res(fee);
                                        quick_send!(state, id, RpcMsg::TotalFee(var)).wait().unwrap();
                                    },
                                    None => {
                                        let err = "failed to retrieve total fee".to_string();
                                        quick_send!(state, id, RpcMsg::Error(err)).wait().unwrap();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        }));
    }

    fn get_next_peer(&self) -> Option<PeerState> {
        let peers = self.peers.lock();
        let len = peers.len();
        if len < 1 { return None }

        let first_count = self.peer_iter_count.fetch_add(1, Ordering::AcqRel);
        loop {
            let count = self.peer_iter_count.fetch_add(1, Ordering::AcqRel);
            let peer = &peers[count % len];
            if peer.connected.load(Ordering::Acquire) { return Some(peer.clone()) }
            if count == first_count { break; }
        }

        None
    }
}

#[derive(Clone)]
struct PeerState {
    tx: ClientSender,
    rx: ClientReceiver,
    connected: Arc<AtomicBool>,
    id: Arc<AtomicUsize>
}
