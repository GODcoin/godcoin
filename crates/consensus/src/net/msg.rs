use super::{Request, Response, Serializable};
use bytes::{BufMut, BytesMut};
use godcoin::serializer::BufRead;
use std::{
    io::{self, Cursor},
    mem::size_of,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msg {
    pub id: u64,
    pub data: MsgKind,
}

impl Serializable<Self> for Msg {
    fn serialize(&self, dst: &mut BytesMut) {
        dst.put_u64(self.id);
        self.data.serialize(dst);
    }

    fn byte_size(&self) -> usize {
        size_of::<u64>() + self.data.byte_size()
    }

    fn deserialize(src: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let id = src.take_u64()?;
        let data = MsgKind::deserialize(src)?;
        Ok(Self { id, data })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MsgKind {
    Handshake(Handshake),
    Request(Request),
    Response(Response),
}

impl Serializable<Self> for MsgKind {
    fn serialize(&self, dst: &mut BytesMut) {
        match self {
            Self::Handshake(hs) => {
                dst.put_u8(0x01);
                hs.serialize(dst);
            }
            Self::Request(req) => {
                dst.put_u8(0x02);
                req.serialize(dst);
            }
            Self::Response(res) => {
                dst.put_u8(0x03);
                res.serialize(dst);
            }
        }
    }

    fn byte_size(&self) -> usize {
        let hint = match self {
            Self::Handshake(hs) => hs.byte_size(),
            Self::Request(req) => req.byte_size(),
            Self::Response(res) => res.byte_size(),
        };
        // Add 1 byte for the tag type
        hint + 1
    }

    fn deserialize(src: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = src.take_u8()?;
        match tag {
            0x01 => Ok(MsgKind::Handshake(Handshake::deserialize(src)?)),
            0x02 => Ok(MsgKind::Request(Request::deserialize(src)?)),
            0x03 => Ok(MsgKind::Response(Response::deserialize(src)?)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid tag type on MsgKind",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handshake {
    pub peer_id: u32,
}

impl Serializable<Self> for Handshake {
    fn serialize(&self, dst: &mut BytesMut) {
        dst.put_u32(self.peer_id);
    }

    fn byte_size(&self) -> usize {
        4
    }

    fn deserialize(src: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let peer_id = src.take_u32()?;
        Ok(Self { peer_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_msg() {
        let msg_a = Msg {
            id: 1234,
            data: MsgKind::Handshake(Handshake { peer_id: 5678 }),
        };
        let mut bytes = BytesMut::with_capacity(msg_a.byte_size());
        msg_a.serialize(&mut bytes);
        verify_byte_len(&bytes, msg_a.byte_size());

        let msg_b = Msg::deserialize(&mut Cursor::new(bytes.as_ref())).unwrap();
        assert_eq!(msg_a, msg_b);
    }

    #[test]
    fn serialize_handshake() {
        let handshake_a = Handshake { peer_id: 1234 };
        let mut bytes = BytesMut::with_capacity(handshake_a.byte_size());
        handshake_a.serialize(&mut bytes);
        verify_byte_len(&bytes, handshake_a.byte_size());

        let handshake_b = Handshake::deserialize(&mut Cursor::new(bytes.as_ref())).unwrap();
        assert_eq!(handshake_a, handshake_b);
    }

    fn verify_byte_len(bytes: &BytesMut, expected_size: usize) {
        assert_eq!(bytes.len(), expected_size);
        assert_eq!(bytes.capacity(), expected_size);
    }
}
