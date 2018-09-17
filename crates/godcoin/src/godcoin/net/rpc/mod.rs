use blockchain::{SignedBlock, Properties};
use tx::TxVariant;
use super::peer::*;

pub mod codec;

#[repr(u8)]
#[derive(Debug)]
pub enum RpcMsgType {
    ERROR = 0,
    EVENT = 1,
    HANDSHAKE = 2,
    PROPERTIES = 3,
    BROADCAST = 4
}

#[derive(Clone, Debug)]
pub enum RpcMsg {
    Error(String),
    Event(RpcEvent),
    Handshake(RpcMsgHandshake),
    Properties(Option<Properties>)
}

#[derive(Clone, Debug)]
pub struct RpcPayload {
    pub id: u32,
    pub msg: Option<RpcMsg>
}

#[derive(Clone, Debug)]
pub struct RpcMsgHandshake {
    pub peer_type: PeerType
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
