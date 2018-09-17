use blockchain::{SignedBlock, Properties};
use tx::TxVariant;
use super::peer::*;

pub mod codec;

#[repr(u8)]
#[derive(Debug)]
pub enum RpcMsgType {
    HANDSHAKE = 0,
    PROPERTIES = 1,
    BROADCAST = 2,
    EVENT = 3
}

#[derive(Clone, Debug)]
pub enum RpcMsg {
    Handshake(RpcMsgHandshake),
    Properties(Option<Properties>),
    Event(RpcEvent)
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
    Tx(Option<TxVariant>),
    Block(Option<SignedBlock>)
}
