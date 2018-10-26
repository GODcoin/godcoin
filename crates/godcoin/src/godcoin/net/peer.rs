use std::{
    fmt,
    io,
    collections::HashMap,
    net::SocketAddr,
    sync::Arc
};
use futures::{
    task,
    Sink,
    Poll,
    Async,
    AsyncSink,
    sync::{mpsc, oneshot},
    stream::Stream
};
use tokio::net::TcpStream;
use tokio_codec::Framed;
use parking_lot::Mutex;
use log::debug;

use super::rpc::*;

type Tx = mpsc::UnboundedSender<RpcPayload>;
type Rx = mpsc::UnboundedReceiver<RpcPayload>;
type RpcFrame = Framed<TcpStream, codec::RpcCodec>;
type ReqMap = HashMap<u32, oneshot::Sender<RpcPayload>>;

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum PeerType {
    NODE = 0,
    WALLET = 1
}

#[derive(Clone)]
pub struct Sender(Tx, Arc<Mutex<ReqMap>>);

impl Sender {
    #[inline]
    pub fn send(&self, payload: RpcPayload) -> oneshot::Receiver<RpcPayload> {
        let id = payload.id;
        self.0.unbounded_send(payload).unwrap();

        let (tx, rx) = oneshot::channel();
        self.1.lock().insert(id, tx);

        rx
    }

    #[inline]
    pub fn send_untracked(&self, payload: RpcPayload) {
        self.0.unbounded_send(payload).unwrap();
    }
}

pub struct Peer {
    pub peer_type: PeerType,
    pub addr: SocketAddr,
    reqs: Arc<Mutex<ReqMap>>,
    tx: Tx,
    rx: Rx,
    frame: RpcFrame
}

impl Peer {
    pub fn new(peer_type: PeerType,
                addr: SocketAddr,
                frame: RpcFrame) -> Peer {
        let (tx, rx) = mpsc::unbounded();
        Peer {
            peer_type,
            addr,
            reqs: Arc::new(Mutex::new(HashMap::with_capacity(32))),
            frame,
            tx,
            rx
        }
    }

    pub fn get_sender(&self) -> Sender {
        Sender(self.tx.clone(), Arc::clone(&self.reqs))
    }
}

impl fmt::Debug for Peer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Peer {{ peer_type: {:?}, addr: {:?} }}",
                &self.peer_type,
                &self.addr)
    }
}

impl Stream for Peer {
    type Item = RpcPayload;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if let Async::Ready(msg) = self.frame.poll()? {
            if let Some(payload) = &msg {
                let tx = self.reqs.lock().remove(&payload.id);
                if let Some(tx) = tx {
                    tx.send(payload.clone()).unwrap();
                }
            }
            return Ok(Async::Ready(msg))
        }

        while let Async::Ready(msg) = self.rx.poll().unwrap() {
            if let Some(msg) = msg {
                let res = self.frame.start_send(msg).unwrap();
                match res {
                    AsyncSink::Ready => {},
                    AsyncSink::NotReady(msg) => {
                        self.tx.unbounded_send(msg).unwrap();
                        task::current().notify();
                        break;
                    }
                }
            }
        }
        self.frame.poll_complete()?;

        Ok(Async::NotReady)
    }
}

impl ::std::ops::Drop for Peer {
    fn drop(&mut self) {
        debug!("Peer dropped: {:?}", self);
    }
}
