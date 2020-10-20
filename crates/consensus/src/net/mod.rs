pub mod msg;
pub mod rpc;

use bytes::{Buf, BufMut, BytesMut};
use std::{fmt, io, mem, num::NonZeroU64};
use tokio_util::codec::{Decoder, Encoder};

pub use msg::*;
pub use rpc::*;

pub trait Serializable<T> {
    fn serialize(&self, dst: &mut BytesMut);
    fn byte_size(&self) -> usize;
    fn deserialize(src: &mut io::Cursor<&[u8]>) -> io::Result<T>;
}

#[derive(Debug, Default)]
pub struct RpcCodec {
    frame_len: Option<NonZeroU64>,
}

impl Encoder<Msg> for RpcCodec {
    type Error = io::Error;

    fn encode(&mut self, data: Msg, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let data_len = data.byte_size();
        dst.reserve(mem::size_of::<u64>() + data_len);
        dst.put_u64(data.byte_size() as u64);
        data.serialize(dst);
        Ok(())
    }
}

impl Decoder for RpcCodec {
    type Item = Msg;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.frame_len.is_none() && src.len() >= mem::size_of::<u64>() {
            let mut src = src.split_to(mem::size_of::<u64>());
            let frame_len = src.get_u64();
            let frame_len = NonZeroU64::new(frame_len).ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, DecodeError::InvalidFrameLen)
            })?;
            self.frame_len = Some(frame_len);
            src.reserve(frame_len.get() as usize);
        }
        match self.frame_len {
            Some(frame_len) if src.len() as u64 >= frame_len.get() => {
                let bytes = src.split_to(frame_len.get() as usize);
                self.frame_len = None;

                let mut cur = io::Cursor::new(bytes.as_ref());
                let msg = Msg::deserialize(&mut cur)?;
                Ok(Some(msg))
            }
            _ => Ok(None),
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum DecodeError {
    InvalidFrameLen,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for DecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode() {
        let mut codec = RpcCodec::default();
        let mut bytes = BytesMut::with_capacity(1024);

        let msg_a = Msg {
            id: 1234,
            data: MsgKind::Handshake(Handshake { peer_id: 5678 }),
        };

        codec.encode(msg_a.clone(), &mut bytes).unwrap();
        assert_eq!(bytes.len(), msg_a.byte_size() + 8);

        let msg_b = codec.decode(&mut bytes).expect("Decode error");
        let msg_b = msg_b.expect("Invalid built frame");
        assert_eq!(msg_a, msg_b);
        assert_eq!(bytes.len(), 0);
    }
}
