use tokio::codec::{Encoder, Decoder};
use std::io::{Error, ErrorKind};
use bytes::{BufMut, BytesMut};

// 5 MiB limit
const MAX_PAYLOAD_LEN: u32 = 5_242_880;

#[derive(Default, Debug)]
pub struct Codec {
    msg_len: u32
}

impl Codec {
    pub fn new() -> Codec {
        Codec::default()
    }
}

impl Encoder for Codec {
    type Item = Vec<u8>;
    type Error = Error;

    fn encode(&mut self, item: Self::Item, buf: &mut BytesMut) -> Result<(), Error> {
        assert!((item.len() as u32) < MAX_PAYLOAD_LEN);
        buf.reserve(4 + item.len());
        buf.put_u32_be(item.len() as u32);
        buf.put_slice(&item);
        Ok(())
    }
}

impl Decoder for Codec {
    type Item = BytesMut;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Error> {
        if self.msg_len == 0 && buf.len() >= 4 {
            let buf = buf.split_to(4);
            self.msg_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
            if self.msg_len > MAX_PAYLOAD_LEN {
                return Err(Error::new(ErrorKind::Other, format!("payload must be <={} bytes", MAX_PAYLOAD_LEN)))
            }
        }
        if self.msg_len != 0 && buf.len() >= self.msg_len as usize {
            let msg_len = self.msg_len;
            self.msg_len -= msg_len;

            let split = buf.split_to(msg_len as usize);
            Ok(Some(split))
        } else {
            Ok(None)
        }
    }
}
