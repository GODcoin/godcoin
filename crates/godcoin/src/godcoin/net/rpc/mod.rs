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
    Properties(Option<Properties>),
    Block(TxRx<u64, SignedBlock>),
    Balance(TxRx<PublicKey, Balance>),
    TotalFee(TxRx<PublicKey, Balance>)
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
pub enum TxRx<A, B> {
    Tx(A),
    Rx(B)
}

#[repr(u8)]
pub enum TxRxType {
    Tx = 0,
    Rx = 1
}
