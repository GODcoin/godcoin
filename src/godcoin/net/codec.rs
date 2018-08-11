use std::io::{Cursor, Error, ErrorKind};
use tokio_codec::{Encoder, Decoder};
use bytes::{Buf, BufMut, BytesMut};

use net::rpc::*;

// 5 MiB limit
const MAX_PAYLOAD_LEN: u32 = 5242880;

pub struct RpcCodec {
    msg_len: u32
}

impl RpcCodec {
    pub fn new() -> RpcCodec {
        RpcCodec { msg_len: 0 }
    }
}

impl Encoder for RpcCodec {
    type Item = RpcTxPayload;
    type Error = Error;

    fn encode(&mut self, pl: Self::Item, buf: &mut BytesMut) -> Result<(), Error> {
        buf.reserve(9 + pl.payload.len());
        debug_assert!((buf.capacity() as u32) < MAX_PAYLOAD_LEN);

        // Include the msg_type as part of the payload length
        buf.put_u32_be((pl.payload.len() as u32) + 5);

        buf.put_u32_be(pl.id);
        buf.put_u8(pl.msg_type as u8);
        buf.put(pl.payload);
        Ok(())
    }
}

impl Decoder for RpcCodec {
    type Item = RpcRxPayload;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Error> {
        if self.msg_len == 0 && buf.len() >= 4 {
            let buf = buf.split_to(4);
            self.msg_len = u32_from_buf!(buf);
            if self.msg_len <= 4 {
                return Err(Error::new(ErrorKind::Other, "payload must be >4 bytes"))
            } else if self.msg_len > MAX_PAYLOAD_LEN {
                return Err(Error::new(ErrorKind::Other, "payload must be <=5242880 bytes"))
            }
        }
        if self.msg_len != 0 && buf.len() >= self.msg_len as usize {
            let mut cur = Cursor::new(buf.split_to(self.msg_len as usize));
            self.msg_len = 0;

            let id = cur.get_u32_be();
            let ty = match cur.get_u8() {
                t if t == RpcMsgType::PROPERTIES as u8 => RpcMsgType::PROPERTIES,
                t if t == RpcMsgType::BROADCAST as u8 => RpcMsgType::BROADCAST,
                _ => return Err(Error::new(ErrorKind::Other, "invalid msg type"))
            };
            Ok(Some(RpcRxPayload {
                id,
                msg_type: ty,
                payload: cur
            }))
        } else {
            Ok(None)
        }
    }
}
