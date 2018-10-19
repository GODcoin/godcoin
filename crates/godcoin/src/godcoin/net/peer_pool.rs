use std::sync::{Arc, atomic::{Ordering, AtomicBool}};
use std::net::SocketAddr;
use parking_lot::Mutex;
use tokio::prelude::*;

use blockchain::Blockchain;
use producer::Producer;

use super::peer::PeerType;
use super::client::*;
use super::rpc::*;

pub struct PeerPool {
    peer_addresses: Vec<SocketAddr>,
    peers: Mutex<Vec<PeerState>>
}

impl PeerPool {
    pub fn new(addrs: &[&str]) -> PeerPool {
        let peer_addresses = addrs.into_iter().map(|s| {
            (*s).parse().map_err(|e| {
                format!("failed to parse address: {} {}", *s, e)
            }).unwrap()
        }).collect::<Vec<SocketAddr>>();
        let peers = Mutex::new(Vec::with_capacity(peer_addresses.len()));

        PeerPool {
            peer_addresses,
            peers
        }
    }

    pub fn start(&self, blockchain: Arc<Blockchain>, producer: Arc<Option<Producer>>) {
        assert!(self.peers.lock().is_empty(), "peer pool already started");
        for addr in self.peer_addresses.clone() {
            let (tx, rx) = connect_loop(addr, PeerType::NODE);
            let state = PeerState {
                tx,
                rx,
                connected: Arc::new(AtomicBool::new(false))
            };
            let blockchain = Arc::clone(&blockchain);
            let producer = Arc::clone(&producer);
            self.handle_client_peer(addr, blockchain, producer, state);
        }
    }

    fn handle_client_peer(&self,
                            addr: SocketAddr,
                            blockchain: Arc<Blockchain>,
                            producer: Arc<Option<Producer>>,
                            state: PeerState) {
        macro_rules! quick_send {
            ($state:expr, $rpc:expr, $msg:expr) => {
                $state.tx.send(ClientEvent::Message(RpcPayload {
                    id: $rpc.id,
                    msg: Some($msg)
                }))
            };
            ($state:expr, $rpc:expr) => {
                $state.tx.send(ClientEvent::Message(RpcPayload {
                    id: $rpc.id,
                    msg: None
                }))
            };
        }

        self.peers.lock().push(state.clone());
        ::tokio::spawn(state.rx.clone().for_each(move |evt| {
            match evt {
                ClientEvent::Connect => {
                    state.connected.store(true, Ordering::Release);
                },
                ClientEvent::Disconnect => {
                    state.connected.store(false, Ordering::Release);
                },
                ClientEvent::Message(rpc) => {
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
                            if let Some(producer) = &*producer {
                                match evt {
                                    RpcEvent::Block(block) => {
                                        let _ = producer.add_block(block);
                                    },
                                    RpcEvent::Tx(tx) => {
                                        let _ = producer.add_tx(tx);
                                    }
                                }
                            }
                        },
                        RpcMsg::Broadcast(tx) => {
                            if let Some(producer) = &*producer {
                                match producer.add_tx(tx) {
                                    Ok(_) => {
                                        quick_send!(state, rpc).wait().unwrap();
                                    },
                                    Err(s) => {
                                        quick_send!(state, rpc, RpcMsg::Error(s)).wait().unwrap();
                                    }
                                }
                            }
                        },
                        RpcMsg::Properties(var) => {
                            if let Some(_) = var.request() {
                                let props = blockchain.get_properties();
                                quick_send!(state, rpc, RpcMsg::Properties(RpcVariant::Res(props))).wait().unwrap();
                            }
                        },
                        RpcMsg::Block(var) => {
                            if let Some(height) = var.request() {
                                let block = match blockchain.get_block(*height) {
                                    Some(block) => Some((&*block).clone()),
                                    None => None
                                };
                                quick_send!(state, rpc, RpcMsg::Block(RpcVariant::Res(block))).wait().unwrap();
                            }
                        },
                        RpcMsg::Balance(var) => {
                            if let Some(addr) = var.request() {
                                let bal = blockchain.get_balance(addr);
                                quick_send!(state, rpc, RpcMsg::Balance(RpcVariant::Res(bal))).wait().unwrap();
                            }
                        },
                        RpcMsg::TotalFee(var) => {
                            if let Some(addr) = var.request() {
                                let fee = blockchain.get_total_fee(addr);
                                match fee {
                                    Some(fee) => {
                                        quick_send!(state, rpc, RpcMsg::TotalFee(RpcVariant::Res(fee))).wait().unwrap();
                                    },
                                    None => {
                                        let err = "failed to retrieve total fee".to_string();
                                        quick_send!(state, rpc, RpcMsg::Error(err)).wait().unwrap();
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
}

#[derive(Clone)]
struct PeerState {
    tx: ClientSender,
    rx: ClientReceiver,
    connected: Arc<AtomicBool>
}
