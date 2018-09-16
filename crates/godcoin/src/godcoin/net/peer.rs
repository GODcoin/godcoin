use futures::{task, Sink, Poll, Async, AsyncSink, sync::mpsc, stream::Stream};
use tokio::net::TcpStream;
use std::net::SocketAddr;
use tokio_codec::Framed;
use std::{fmt, io};

use super::rpc::*;

type Tx = mpsc::UnboundedSender<RpcPayload>;
type Rx = mpsc::UnboundedReceiver<RpcPayload>;
type RpcFrame = Framed<TcpStream, codec::RpcCodec>;

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum PeerType {
    NODE = 0,
    WALLET = 1
}

#[derive(Clone)]
pub struct Sender(Tx);

impl Sender {
    #[inline]
    pub fn send(&self, payload: RpcPayload) {
        self.0.unbounded_send(payload).unwrap();
    }
}

pub struct Peer {
    pub peer_type: PeerType,
    pub addr: SocketAddr,
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
            frame,
            addr,
            tx,
            rx
        }
    }

    pub fn get_sender(&self) -> Sender {
        Sender(self.tx.clone())
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
