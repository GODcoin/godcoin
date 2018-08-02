use std::io::Cursor;
use bytes::BytesMut;

#[repr(u8)]
#[derive(Debug)]
pub enum RpcMsgType {
    PROPERTIES = 0,
    BROADCAST = 1
}

#[derive(Debug)]
pub struct RpcTxPayload {
    pub msg_type: RpcMsgType,
    pub id: u32,
    pub payload: Vec<u8>
}

#[derive(Debug)]
pub struct RpcRxPayload {
    pub msg_type: RpcMsgType,
    pub id: u32,
    pub payload: Cursor<BytesMut>
}
