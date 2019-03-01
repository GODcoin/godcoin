use tokio::codec::{Encoder, Decoder};
use std::io::{Error, ErrorKind};
use bytes::{BufMut, BytesMut};

// 5 MiB limit
const MAX_PAYLOAD_LEN: u32 = 5_242_880;

#[derive(Debug)]
pub struct Payload {
    id: BytesMut,
    msg: BytesMut
}

#[derive(Default, Debug)]
pub struct Codec {
    id_len: u32,
    msg_len: u32
}

impl Codec {
    pub fn new() -> Codec {
        Codec::default()
    }
}

impl Encoder for Codec {
    type Item = Payload;
    type Error = Error;

    fn encode(&mut self, item: Self::Item, buf: &mut BytesMut) -> Result<(), Error> {
        {
            let total_len = item.id.len() + item.msg.len() + 8;
            assert!((total_len as u32) < MAX_PAYLOAD_LEN);
            buf.reserve(8 + total_len);
        }
        buf.put_u32_be(item.id.len() as u32);
        buf.put_u32_be(item.msg.len() as u32);
        buf.put(item.id);
        buf.put(item.msg);
        Ok(())
    }
}

impl Decoder for Codec {
    type Item = Payload;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Error> {
        if self.id_len == 0 && self.msg_len == 0 && buf.len() >= 8 {
            let buf = buf.split_to(8);
            self.id_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
            self.msg_len = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
            if self.id_len + self.msg_len > MAX_PAYLOAD_LEN {
                return Err(Error::new(ErrorKind::Other, format!("payload must be <={} bytes", MAX_PAYLOAD_LEN)))
            }
        }
        if self.id_len != 0 && self.msg_len != 0
                && buf.len() >= (self.id_len + self.msg_len) as usize {
            let id_len = self.id_len;
            self.id_len = 0;

            let msg_len = self.msg_len;
            self.msg_len = 0;

            let id = buf.split_to(id_len as usize);
            let msg = buf.split_to(msg_len as usize);
            Ok(Some(Payload { id, msg }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_decode {
        ($codec:expr, $bytes:expr, $id:expr, $msg:expr) => {
            {
                let res = $codec.decode(&mut $bytes).unwrap();
                assert!(res.is_some(), "decoding failure, expected Ok");
                let res = res.expect("decoding failure, expected payload");
                assert_eq!(res.id, $id, "id mismatch");
                assert_eq!(res.msg, $msg, "msg mismatch");
            }
        };
    }

    #[test]
    fn test_encode_full_frame() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::new();
        codec.encode(Payload {
            id: vec![0, 1, 2, 3, 4, 5].into(),
            msg: vec![255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245].into()
        }, &mut bytes).unwrap();
        assert_eq!(bytes, vec![
            0, 0, 0, 6, // ID len
            0, 0, 0, 11, // Msg len
            0, 1, 2, 3, 4, 5, // ID
            255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245 // Msg
        ]);
    }

    #[test]
    fn test_decode_full_frame() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::from(vec![
            0, 0, 0, 6, // ID len
            0, 0, 0, 11, // Msg len
            0, 1, 2, 3, 4, 5, // ID
            255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245 // Msg
        ]);

        assert_decode!(codec, bytes,
                        vec![0, 1, 2, 3, 4, 5],
                        vec![255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245]);
    }

    #[test]
    fn test_decode_multipart_frame() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::from(vec![
            0, 0, 0, 6, // ID len
            0, 0, 0, 11, // Msg len
            0, 1, 2, 3, 4, 5, // ID
            255, 254, 253, 252, 251, 250, 249, 248 // Msg
        ]);
        let res = codec.decode(&mut bytes).unwrap();
        assert!(res.is_none());

        bytes.reserve(3);
        bytes.put_slice(&vec![247, 246, 245]);
        assert_decode!(codec, bytes,
                        vec![0, 1, 2, 3, 4, 5],
                        vec![255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245]);
    }

    #[test]
    fn test_decode_multiple_multipart_frames() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::from(vec![
            0, 0, 0, 6, // ID len
            0, 0, 0, 11, // Msg len
            0, 1, 2, 3, 4, 5, // ID
            255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245, // Msg
            0, 0, 0, 3 // ID len
        ]);
        assert_decode!(codec, bytes,
                vec![0, 1, 2, 3, 4, 5],
                vec![255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245]);

        // Ensure the codec data is correct
        assert_eq!(bytes.len(), 4);
        assert_eq!(codec.id_len, 0);
        assert_eq!(codec.msg_len, 0);

        bytes.reserve(4);
        bytes.put_slice(&vec![0, 0, 0, 5]); // Write msg len
        let res = codec.decode(&mut bytes).unwrap();
        assert!(res.is_none());

        assert_eq!(bytes.len(), 0);
        assert_eq!(codec.id_len, 3);
        assert_eq!(codec.msg_len, 5);

        bytes.reserve(8);
        bytes.put_slice(&vec![
            1, 1, 1, // ID
            2, 2, 2, 2, 2 // Msg
        ]);
        assert_decode!(codec, bytes,
                vec![1, 1, 1],
                vec![2, 2, 2, 2, 2]);
    }

    #[test]
    fn test_decode_multiple_full_frames() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::from(vec![
            // First payload
            0, 0, 0, 6, // ID len
            0, 0, 0, 11, // Msg len
            0, 1, 2, 3, 4, 5, // ID
            255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245, // Msg
            // Second payload
            0, 0, 0, 3,
            0, 0, 0, 4,
            1, 1, 1,
            2, 2, 2, 2,
            // Third payload
            0, 0, 0, 4,
            0, 0, 0, 5,
            3, 3, 3, 3,
            4, 4, 4, 4, 4
        ]);

        assert_decode!(codec, bytes,
                vec![0, 1, 2, 3, 4, 5],
                vec![255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245]);

        assert_decode!(codec, bytes,
                vec![1, 1, 1],
                vec![2, 2, 2, 2]);

        assert_decode!(codec, bytes,
                vec![3, 3, 3, 3],
                vec![4, 4, 4, 4, 4]);

        assert_eq!(bytes.len(), 0);
        assert_eq!(codec.id_len, 0);
        assert_eq!(codec.msg_len, 0);
    }
}
