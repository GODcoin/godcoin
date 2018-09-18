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
    Properties(RpcVariant<(), Properties>),
    Block(RpcVariant<u64, SignedBlock>),
    Balance(RpcVariant<PublicKey, Balance>),
    TotalFee(RpcVariant<PublicKey, Balance>)
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
pub enum RpcVariant<A, B> {
    Req(A),
    Res(B)
}

impl<A, B> RpcVariant<A, B> {
    #[inline]
    pub fn request(&self) -> Option<&A> {
        match self {
            RpcVariant::Req(a) => Some(&a),
            RpcVariant::Res(_) => None
        }
    }

    #[inline]
    pub fn response(&self) -> Option<&B> {
        match self {
            RpcVariant::Req(_) => None,
            RpcVariant::Res(b) => Some(&b)
        }
    }
}

#[repr(u8)]
pub enum RpcVariantType {
    Req = 0,
    Res = 1
}
