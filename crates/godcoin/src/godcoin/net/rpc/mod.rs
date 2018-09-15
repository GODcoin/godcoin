use blockchain::Properties;
use super::peer::*;

pub mod codec;

#[repr(u8)]
#[derive(Debug)]
pub enum RpcMsgType {
    HANDSHAKE = 0,
    PROPERTIES = 1,
    BROADCAST = 2
}

#[derive(Clone, Debug)]
pub enum RpcMsg {
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
    pub client_type: ClientType
}
