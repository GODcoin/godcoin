use blockchain::{SignedBlock, Properties};
use crypto::PublicKey;
use asset::Balance;
use tx::TxVariant;
use super::peer::*;

pub mod codec;

#[repr(u8)]
#[derive(Debug)]
pub enum RpcMsgType {
    Error = 0,
    Event = 1,
    Handshake = 2,
    Broadcast = 3,
    Properties = 4,
    Block = 5,
    Balance = 6,
    TotalFee = 7
}

#[derive(Clone, Debug)]
pub enum RpcMsg {
    Error(String),
    Event(RpcEvent),
    Handshake(PeerType),
    Broadcast(TxVariant),
    Properties(IO<(), Properties>),
    Block(IO<u64, SignedBlock>),
    Balance(IO<PublicKey, Balance>),
    TotalFee(IO<PublicKey, Balance>)
}

#[derive(Clone, Debug)]
pub struct RpcPayload {
    pub id: u32,
    pub msg: Option<RpcMsg>
}

#[repr(u8)]
pub enum RpcEventType {
    TX = 0,
    BLOCK = 1
}

#[derive(Clone, Debug)]
pub enum RpcEvent {
    Tx(TxVariant),
    Block(SignedBlock)
}

#[derive(Clone, Debug)]
pub enum IO<A, B> {
    In(A),
    Out(B)
}

impl<A, B> IO<A, B> {
    #[inline]
    pub fn input(&self) -> Option<&A> {
        match self {
            IO::In(a) => Some(&a),
            IO::Out(_) => None
        }
    }

    #[inline]
    pub fn output(&self) -> Option<&B> {
        match self {
            IO::In(_) => None,
            IO::Out(b) => Some(&b)
        }
    }
}

#[repr(u8)]
pub enum IoType {
    In = 0,
    Out = 1
}
