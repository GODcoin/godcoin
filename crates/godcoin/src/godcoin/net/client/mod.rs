use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::io::{Error, ErrorKind};
use tokio::net::TcpStream;
use std::net::SocketAddr;
use tokio_codec::Framed;
use tokio::timer::Delay;
use tokio::prelude::*;
use rand;

use super::peer::*;
use super::rpc::*;
use fut_util::*;

mod state;

pub mod event;
use self::event::*;

///
/// Connects to the `SocketAddr` with the specified `PeerType`.
///
pub fn connect(addr: SocketAddr, peer_type: PeerType) -> impl Future<Item = Peer, Error = Error> {
    TcpStream::connect(&addr).and_then(move |stream| {
        let frame = Framed::new(stream, codec::RpcCodec::new());
        let msg = RpcPayload {
            id: 0,
            msg: Some(RpcMsg::Handshake(peer_type))
        };

        debug!("[{}] Sending handshake: {:?}", addr, &msg);
        frame.send(msg)
    }).and_then(move |frame| {
        let (resp, frame) = frame.into_future().map_err(|(e, _)| e).wait()?;
        let resp = resp.ok_or_else(|| Error::from(ErrorKind::UnexpectedEof))?;
        debug!("[{}] Received handshake message: {:?}", addr, &resp);
        if resp.id != 0 {
            return Err(Error::new(ErrorKind::InvalidData, "expected id to be 0"))
        } else if resp.msg.is_some() {
            return Err(Error::new(ErrorKind::InvalidData, "expected msg to be empty"))
        }
        Ok(Peer::new(peer_type, addr, frame))
    })
}

///
/// Creates a persistent connection to the `SocketAddr` provided. Events are
/// emitted via the `ClientReceiver`. When sending a `ClientEvent::Disconnect`
/// event the returned `ClientReceiver` stream will be closed.
///
/// See [`ClientEvent`] for the type of events to send and receive.
///
/// [`ClientEvent`]: ./event/enum.ClientEvent.html
///
/// # Panics
///
/// When attempting to send a message that requires the connection to be open.
///
pub fn connect_loop(addr: SocketAddr, peer_type: PeerType) -> (ClientSender, ClientReceiver) {
    let (out_tx, out_rx) = channel::unbounded();
    let (in_tx, in_rx) = channel::unbounded();
    let state = state::ConnectState::new(addr, peer_type);
    start_connect_loop(state.clone(), out_tx, 0);

    ::tokio::spawn(in_rx.for_each({
        let stay_connected = Arc::clone(&state.stay_connected);
        let inner = Arc::clone(&state.inner);
        move |msg| {
            match msg {
                ClientEvent::Message(msg) => {
                    let guard = inner.lock();
                    let inner = guard.borrow();
                    let inner = inner.as_ref().expect("must be connected to send msg");
                    inner.sender.send(msg);
                },
                ClientEvent::Connect => panic!("cannot connect from event channel"),
                ClientEvent::Disconnect => {
                    stay_connected.store(false, Ordering::Release);
                    let guard = inner.lock();
                    let inner = guard.borrow();
                    let inner = inner.as_ref().expect("must be connected to disconnect");
                    inner.notifier.send(());
                }
            }
            Ok(())
        }
    }));

    (in_tx, out_rx)
}

fn start_connect_loop(state: state::ConnectState, out_tx: ClientSender, mut tries: u8) {
    if !state.stay_connected.load(Ordering::Acquire) { return }
    let c = connect(state.addr, state.peer_type);
    ::tokio::spawn(c.and_then({
        let out_tx = out_tx.clone();
        let state = state.clone();
        move |peer| {
            if !state.stay_connected.load(Ordering::Acquire) { return Ok(()) }
            info!("[{}] Connected to peer", state.addr);

            let connected = Arc::new(AtomicBool::new(true));
            out_tx.send(ClientEvent::Connect);
            tries = 0;

            let rx = {
                let (tx, rx) = channel::unbounded();
                let guard = state.inner.lock();
                *guard.borrow_mut() = Some(state::InternalState {
                    sender: peer.get_sender(),
                    notifier: tx
                });
                rx.map_err(|_| {
                    Error::new(ErrorKind::Other, "rx error")
                })
            };

            ::tokio::spawn(ZipEither::new(peer, rx).take_while({
                let stay_connected = Arc::clone(&state.stay_connected);
                let connected = Arc::clone(&connected);
                move |_| {
                    let stay_connected = stay_connected.load(Ordering::Acquire);
                    let connected = connected.load(Ordering::Acquire);
                    Ok(stay_connected && connected)
                }
            }).for_each({
                let out_tx = out_tx.clone();
                move |(rpc, _)| {
                    if let Some(rpc) = rpc {
                        out_tx.send(ClientEvent::Message(rpc));
                    }
                    Ok(())
                }
            }).and_then({
                let addr = state.addr;
                move |_| {
                    warn!("[{}] Peer disconnected", addr);
                    Ok(())
                }
            }).or_else({
                let addr = state.addr;
                move |e| -> Result<_, ()> {
                    error!("[{}] Peer frame processing error: {}", addr, e);
                    Ok(())
                }
            }).and_then({
                let stay_connected = Arc::clone(&state.stay_connected);
                move |_| {
                    out_tx.send(ClientEvent::Disconnect);
                    {
                        let guard = state.inner.lock();
                        *guard.borrow_mut() = None;
                    }
                    if stay_connected.load(Ordering::Acquire) {
                        try_connect(state, out_tx, tries.saturating_add(1));
                    }
                    Ok(())
                }
            }));

            Ok(())
        }
    }).map_err(move |e| {
        if state.stay_connected.load(Ordering::Acquire) {
            error!("[{}] Failed to connect to peer: {:?}", state.addr, e);
            try_connect(state, out_tx, tries.saturating_add(1));
        }
    }));
}

fn try_connect(state: state::ConnectState, out_tx: ClientSender, tries: u8) {
    use ::std::time::{Duration, Instant};

    let ms = backoff(tries);
    info!("[{}] Attempting to reconnect to peer in {} ms (tries: {})", state.addr, ms, tries);
    let d = Instant::now() + Duration::from_millis(ms);
    ::tokio::spawn(Delay::new(d).map(move |_| {
        start_connect_loop(state, out_tx, tries);
    }).map_err(|e| {
        error!("Connection timer error: {}", e);
    }));
}

fn backoff(tries: u8) -> u64 {
    use rand::Rng;
    let tries = f64::from(tries);
    let max = 15000f64;
    let rand = rand::thread_rng().gen_range::<f64>(0.2f64, 1f64);
    max.min((1.25f64.powf(tries) * 1000f64 * rand).floor()) as u64
}
