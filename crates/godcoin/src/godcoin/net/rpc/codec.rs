use std::io::{Cursor, Error, ErrorKind};
use tokio_codec::{Encoder, Decoder};
use bytes::{Buf, BufMut, BytesMut};
use std::mem::size_of;
use serializer::*;

use blockchain::Properties;
use net::rpc::*;

// 5 MiB limit
const MAX_PAYLOAD_LEN: u32 = 5_242_880;

#[derive(Default)]
pub struct RpcCodec {
    msg_len: u32
}

impl RpcCodec {
    pub fn new() -> RpcCodec {
        RpcCodec::default()
    }
}

impl Encoder for RpcCodec {
    type Item = RpcPayload;
    type Error = Error;

    fn encode(&mut self, pl: Self::Item, buf: &mut BytesMut) -> Result<(), Error> {
        let mut payload = Vec::<u8>::with_capacity(1024);
        payload.push_u32(pl.id);
        if let Some(msg) = pl.msg {
            match msg {
                RpcMsg::Handshake(hs) => {
                    payload.push(RpcMsgType::HANDSHAKE as u8);
                    payload.push(hs.client_type as u8);
                },
                RpcMsg::Properties(props) => {
                    payload.push(RpcMsgType::PROPERTIES as u8);
                    if let Some(props) = props {
                        payload.push_u64(props.height);
                    }
                }
            }
        }

        buf.reserve(4 + payload.len());
        buf.put_u32_be(4 + (payload.len() as u32));
        buf.put_slice(&payload);
        debug_assert!((buf.capacity() as u32) < MAX_PAYLOAD_LEN);
        let mut v = Vec::<u8>::with_capacity(buf.len());
        v.extend_from_slice(buf);
        Ok(())
    }
}

impl Decoder for RpcCodec {
    type Item = RpcPayload;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Error> {
        if self.msg_len == 0 && buf.len() >= 4 {
            let buf = buf.split_to(4);
            self.msg_len = u32_from_buf!(buf);
            if self.msg_len <= 4 {
                return Err(Error::new(ErrorKind::Other, "payload must be >4 bytes"))
            } else if self.msg_len > MAX_PAYLOAD_LEN {
                return Err(Error::new(ErrorKind::Other, format!("payload must be <={} bytes", MAX_PAYLOAD_LEN)))
            }
            self.msg_len -= 4;
        }
        if self.msg_len != 0 && buf.len() >= self.msg_len as usize {
            let msg_len = self.msg_len;
            let mut cur = Cursor::new(buf.split_to(msg_len as usize));
            self.msg_len = 0;

            let id = cur.get_u32_be();
            if msg_len == 4 {
                return Ok(Some(RpcPayload {
                    id,
                    msg: None
                }))
            }

            let msg = match cur.get_u8() {
                t if t == RpcMsgType::HANDSHAKE as u8 => {
                    let client_type = match cur.get_u8() {
                        t if t == ClientType::NODE as u8 => ClientType::NODE,
                        t if t == ClientType::WALLET as u8 => ClientType::WALLET,
                        _ => return Err(Error::new(ErrorKind::Other, "invalid client type"))
                    };
                    RpcMsg::Handshake(RpcMsgHandshake {
                        client_type
                    })
                },
                t if t == RpcMsgType::PROPERTIES as u8 => {
                    if u64::from(msg_len) - cur.position() >= size_of::<u64>() as u64 {
                        let height = cur.get_u64_be();
                        RpcMsg::Properties(Some(Properties {
                            height
                        }))
                    } else {
                        RpcMsg::Properties(None)
                    }
                },
                //t if t == RpcMsgType::BROADCAST as u8 => RpcMsg::Broadcast,
                _ => return Err(Error::new(ErrorKind::Other, "invalid msg type"))
            };

            Ok(Some(RpcPayload {
                id,
                msg: Some(msg)
            }))
        } else {
            Ok(None)
        }
    }
}
