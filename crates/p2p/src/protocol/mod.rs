use actix::Message;
use bytes::BytesMut;

pub mod codec;

pub use codec::*;

#[derive(Debug)]
#[repr(u8)]
enum ProtocolMsgId {
    Payload = 1,
    Disconnect = 2,
}

#[derive(Debug, PartialEq, Message)]
pub enum ProtocolMsg {
    Payload(Payload),
    Disconnect(String),
}

#[derive(Clone, Debug, PartialEq, Message)]
pub struct Payload {
    pub id: BytesMut,
    pub msg: BytesMut,
}

impl ProtocolMsg {
    pub fn decode(mut bytes: BytesMut) -> Option<ProtocolMsg> {
        if bytes.len() < 1 {
            return None;
        }

        let msg_type = bytes.split_to(1)[0];
        match msg_type {
            t if t == ProtocolMsgId::Payload as u8 => {
                if bytes.len() < 5 {
                    return None;
                }

                let (id_len, msg_len) = {
                    let split_bytes = bytes.split_to(5);
                    let id_len = split_bytes[0] as usize;
                    let msg_len = ((u32::from(split_bytes[1]) << 24)
                        + (u32::from(split_bytes[2]) << 16)
                        + (u32::from(split_bytes[3]) << 8)
                        + u32::from(split_bytes[4])) as usize;
                    (id_len, msg_len)
                };

                if bytes.len() < (usize::from(id_len) + msg_len as usize) {
                    return None;
                }

                let id = bytes.split_to(id_len);
                let msg = bytes.split_to(msg_len);
                Some(ProtocolMsg::Payload(Payload { id, msg }))
            }
            t if t == ProtocolMsgId::Payload as u8 => {
                if bytes.len() < 1 {
                    return None;
                }

                let reason_len = bytes.split_to(1)[0] as usize;
                if bytes.len() < reason_len {
                    return None;
                }

                let reason_bytes = bytes.split_to(reason_len).to_vec();
                let reason = String::from_utf8(reason_bytes).ok()?;
                Some(ProtocolMsg::Disconnect(reason))
            }
            _ => None,
        }
    }
}
