use super::rpc::codec::RpcCodec;
use super::{peer::*, rpc::*};

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use log::{info, warn, error, debug};
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;

use tokio::net::TcpListener;
use tokio_codec::Framed;
use tokio::prelude::*;

use crate::blockchain::Blockchain;
use crate::producer::Minter;
use crate::fut_util::*;

pub fn start(addr: SocketAddr, blockchain: Arc<Blockchain>, minter: Arc<Option<Minter>>) {
    let listener = TcpListener::bind(&addr).unwrap();
    info!("Server binded to {:?}", &addr);
    let server = listener.incoming().for_each(move |socket| {
        let addr = socket.peer_addr().unwrap();
        info!("[{}] Accepted connection", addr);

        let frame = Framed::new(socket, RpcCodec::new());
        let hs = frame.into_future()
                        .map_err(|(e, _)| e)
                        .and_then(|(data, frame)| {
                            if let Some(data) = data { return Ok((data, frame)) }
                            Err(Error::from(ErrorKind::UnexpectedEof))
                        });
        let hs = hs.and_then(|(data, frame)| {
            if data.id != 0 {
                return Err(Error::new(ErrorKind::InvalidData, "id must be 0"))
            }
            if let Some(msg) = data.msg {
                let peer_type = match msg {
                    RpcMsg::Handshake(peer_type) => peer_type,
                    _ => return Err(Error::new(ErrorKind::InvalidData, "expected handshake msg"))
                };
                Ok((peer_type, frame))
            } else {
                Err(Error::new(ErrorKind::InvalidData, "expected handshake msg"))
            }
        });

        let blockchain = Arc::clone(&blockchain);
        let minter = Arc::clone(&minter);
        let client = hs.and_then(move |(peer_type, frame)| {
            let peer = Peer::new(peer_type, addr, frame);
            debug!("Handshake from client completed: {:?}", peer);
            peer.get_sender().send_untracked(RpcPayload {
                id: 0,
                msg: None
            });
            handle_peer(peer, blockchain, minter);
            Ok(())
        });

        tokio::spawn(client.map_err(move |e| {
            debug!("[{}] Handshake error: {:?}", addr, e);
        }));
        Ok(())
    }).map_err(|err| {
        error!("Server accept error: {:?}", err);
    });
    tokio::spawn(server);
}

fn handle_peer(peer: Peer, blockchain: Arc<Blockchain>, minter: Arc<Option<Minter>>) {
    macro_rules! quick_send {
        ($sender:expr, $rpc:expr, $msg:expr) => {
            $sender.send_untracked(RpcPayload { id: $rpc.id, msg: Some($msg) });
        };
        ($sender:expr, $rpc:expr) => {
            $sender.send_untracked(RpcPayload { id: $rpc.id, msg: None });
        };
    }

    let (tx, rx) = {
        let (tx, rx) = channel::unbounded();
        (tx, rx.map_err(|_| {
            Error::new(ErrorKind::Other, "rx error")
        }))
    };

    let addr = peer.addr;
    let force_close = Arc::new(AtomicBool::new(false));
    let sender = peer.get_sender();
    tokio::spawn(ZipEither::new(peer, rx).take_while({
        let force_close = Arc::clone(&force_close);
        move |_| {
            Ok(!force_close.load(Ordering::Acquire))
        }
    }).for_each(move |(rpc, _)| {
        let rpc = match rpc {
            Some(rpc) => rpc,
            None => return Ok(())
        };
        let msg = match rpc.msg {
            Some(msg) => msg,
            None => return Ok(())
        };
        match msg {
            RpcMsg::Handshake(_) => {
                warn!("[{}] Invalid handshake message sent from peer", addr);
                force_close.store(true, Ordering::Release);
                tx.send(())
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
                            quick_send!(sender, rpc);
                        },
                        Err(s) => {
                            quick_send!(sender, rpc, RpcMsg::Error(s));
                        }
                    }
                }
            },
            RpcMsg::Properties(var) => {
                if var.req().is_some() {
                    let props = blockchain.get_properties();
                    quick_send!(sender, rpc, RpcMsg::Properties(RpcVariant::Res(props)));
                }
            },
            RpcMsg::Block(var) => {
                if let Some(height) = var.req() {
                    let block = match blockchain.get_block(height) {
                        Some(block) => Some((&*block).clone()),
                        None => None
                    };
                    quick_send!(sender, rpc, RpcMsg::Block(Box::new(RpcVariant::Res(block))));
                }
            },
            RpcMsg::Balance(var) => {
                if let Some(addr) = var.req() {
                    let bal = blockchain.get_balance(&addr);
                    quick_send!(sender, rpc, RpcMsg::Balance(RpcVariant::Res(bal)));
                }
            },
            RpcMsg::TotalFee(var) => {
                if let Some(addr) = var.req() {
                    let fee = blockchain.get_total_fee(&addr);
                    match fee {
                        Some(fee) => {
                            quick_send!(sender, rpc, RpcMsg::TotalFee(RpcVariant::Res(fee)));
                        },
                        None => {
                            let err = "failed to retrieve total fee".to_string();
                            quick_send!(sender, rpc, RpcMsg::Error(err));
                        }
                    }
                }
            }
        }

        Ok(())
    }).and_then(move |_| {
        warn!("[{}] Client disconnected", addr);
        Ok(())
    }).map_err(move |e| {
        debug!("[{}] Error handling frame from client: {:?}", addr, e);
    }));
}
