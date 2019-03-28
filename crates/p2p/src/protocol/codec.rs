use super::*;
use bytes::{BufMut, BytesMut};
use std::{
    io::{Error, ErrorKind},
    num::NonZeroU8,
};
use tokio::codec::{Decoder, Encoder};

// 5 MiB limit
const MAX_MSG_LEN: u32 = 5_242_880;
// 512 bits should be way more than enough
const MAX_ID_LEN: u8 = 64;

#[derive(Default, Debug)]
pub struct Codec {
    id_len: u8,
    msg_type: Option<NonZeroU8>,
    msg_len: u32,
}

impl Codec {
    pub fn new() -> Codec {
        Codec::default()
    }
}

impl Codec {
    fn reset(&mut self) {
        self.id_len = 0;
        self.msg_type = None;
        self.msg_len = 0;
    }
}

impl Encoder for Codec {
    type Item = ProtocolMsg;
    type Error = Error;

    fn encode(&mut self, item: Self::Item, buf: &mut BytesMut) -> Result<(), Error> {
        match item {
            ProtocolMsg::Payload(pl) => {
                assert!(pl.id.len() < usize::from(MAX_ID_LEN));
                assert!((pl.msg.len() as u32) < MAX_MSG_LEN);
                buf.reserve(pl.id.len() + pl.msg.len() + 6);
                buf.put_u8(ProtocolMsgId::Payload as u8);
                buf.put_u8(pl.id.len() as u8);
                buf.put_u32_be(pl.msg.len() as u32);
                buf.put(pl.id);
                buf.put(pl.msg);
            }
            ProtocolMsg::Disconnect(reason) => {
                assert!(reason.len() <= usize::from(std::u8::MAX));
                buf.reserve(reason.len() + 1);
                buf.put_u8(ProtocolMsgId::Disconnect as u8);
                buf.put_u8(reason.len() as u8);
                buf.put_slice(reason.as_bytes());
            }
        }
        Ok(())
    }
}

impl Decoder for Codec {
    type Item = ProtocolMsg;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Error> {
        if self.msg_type.is_none() && buf.len() > 0 {
            let buf = buf.split_to(1);
            self.msg_type = NonZeroU8::new(buf[0]);
            if self.msg_type.is_none() {
                return Err(Error::new(ErrorKind::Other, "invalid msg_type"));
            }
        }
        if let Some(msg_type) = self.msg_type {
            match msg_type.get() {
                t if t == ProtocolMsgId::Payload as u8 => {
                    if self.id_len == 0 && buf.len() >= 5 {
                        let buf = buf.split_to(5);
                        self.id_len = buf[0];
                        self.msg_len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);
                        if self.id_len == 0 || self.id_len > MAX_ID_LEN {
                            return Err(Error::new(
                                ErrorKind::Other,
                                format!("id must be >0 && <={} bytes", MAX_ID_LEN),
                            ));
                        } else if self.msg_len > MAX_MSG_LEN {
                            return Err(Error::new(
                                ErrorKind::Other,
                                format!("msg must be <={} bytes", MAX_MSG_LEN),
                            ));
                        }
                    }

                    if self.id_len != 0
                        && buf.len() >= usize::from(self.id_len) + (self.msg_len as usize)
                    {
                        let id = buf.split_to(self.id_len as usize);
                        let msg = buf.split_to(self.msg_len as usize);
                        self.reset();
                        Ok(Some(ProtocolMsg::Payload(Payload { id, msg })))
                    } else {
                        Ok(None)
                    }
                }
                t if t == ProtocolMsgId::Disconnect as u8 => {
                    if self.msg_len == 0 && buf.len() > 1 {
                        let buf = buf.split_to(1);
                        self.msg_len = u32::from(buf[0]);
                        if self.msg_len == 0 {
                            self.reset();
                            return Ok(Some(ProtocolMsg::Disconnect(String::new())));
                        }
                    }
                    if self.msg_len != 0 && buf.len() >= self.msg_len as usize {
                        let buf = buf.split_to(self.msg_len as usize);
                        let s = String::from_utf8_lossy(&buf).into_owned();
                        let m = ProtocolMsg::Disconnect(s);
                        self.reset();
                        return Ok(Some(m));
                    } else {
                        Ok(None)
                    }
                }
                _ => Err(Error::new(ErrorKind::Other, "invalid msg_type")),
            }
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    fn assert_decode(codec: &mut Codec, bytes: &mut BytesMut, msg: ProtocolMsg) {
        let res = codec.decode(bytes).unwrap();
        assert!(res.is_some(), "decoding failure, expected Ok");
        let res = res.expect("decoding failure, expected payload");
        assert_eq!(res, msg, "msg mismatch");
    }

    #[test]
    fn test_max_id_len() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::from(vec![1, MAX_ID_LEN + 1, 0, 0, 0, 0]);
        let err = codec.decode(&mut bytes).unwrap_err();
        assert_eq!(
            err.description(),
            format!("id must be >0 && <={} bytes", MAX_ID_LEN)
        );
    }

    #[test]
    fn test_empty_id_len() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::from(vec![1, 0, 0, 0, 0, 0]);
        let err = codec.decode(&mut bytes).unwrap_err();
        assert_eq!(
            err.description(),
            format!("id must be >0 && <={} bytes", MAX_ID_LEN)
        );
    }

    #[test]
    fn test_max_msg_len() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::from(vec![1, 1, 255, 255, 255, 255]);
        let err = codec.decode(&mut bytes).unwrap_err();
        assert_eq!(
            err.description(),
            format!("msg must be <={} bytes", MAX_MSG_LEN)
        );
    }

    #[test]
    fn test_encode_full_frame() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::new();
        codec
            .encode(
                ProtocolMsg::Payload(Payload {
                    id: vec![0, 1, 2, 3, 4, 5].into(),
                    msg: vec![255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245].into(),
                }),
                &mut bytes,
            )
            .unwrap();
        assert_eq!(
            bytes,
            vec![
                1, // Message type
                6, // ID len
                0, 0, 0, 11, // Msg len
                0, 1, 2, 3, 4, 5, // ID
                255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245 // Msg
            ]
        );
    }

    #[test]
    fn test_decode_empty_msg_frame() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::from(vec![
            1, // Message type
            4, // ID len
            0, 0, 0, 0, // Msg len
            0, 1, 2, 3, // ID
        ]);

        assert_decode(
            &mut codec,
            &mut bytes,
            ProtocolMsg::Payload(Payload {
                id: BytesMut::from(vec![0, 1, 2, 3]),
                msg: BytesMut::new(),
            }),
        );
    }

    #[test]
    fn test_decode_full_frame() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::from(vec![
            1, // Message type
            6, // ID len
            0, 0, 0, 11, // Msg len
            0, 1, 2, 3, 4, 5, // ID
            255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245, // Msg
        ]);

        assert_decode(
            &mut codec,
            &mut bytes,
            ProtocolMsg::Payload(Payload {
                id: BytesMut::from(vec![0, 1, 2, 3, 4, 5]),
                msg: BytesMut::from(vec![255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245]),
            }),
        );
    }

    #[test]
    fn test_decode_multipart_frame() {
        let mut codec = Codec::new();
        let mut bytes = BytesMut::from(vec![
            1, // Message type
            6, // ID len
            0, 0, 0, 11, // Msg len
            0, 1, 2, 3, 4, 5, // ID
            255, 254, 253, 252, 251, 250, 249, 248, // Msg
        ]);
        let res = codec.decode(&mut bytes).unwrap();
        assert!(res.is_none());

        bytes.reserve(3);
        bytes.put_slice(&vec![247, 246, 245]);
        assert_decode(
            &mut codec,
            &mut bytes,
            ProtocolMsg::Payload(Payload {
                id: BytesMut::from(vec![0, 1, 2, 3, 4, 5]),
                msg: BytesMut::from(vec![255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245]),
            }),
        );
    }

    #[test]
    fn test_decode_multiple_multipart_frames() {
        let mut codec = Codec::new();
        #[rustfmt::skip]
        let mut bytes = BytesMut::from(vec![
            1, // Message type
            6, // ID len
            0, 0, 0, 11, // Msg len
            0, 1, 2, 3, 4, 5, // ID
            255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245, // Msg
            1, // Message type
            3, // ID len
        ]);
        assert_decode(
            &mut codec,
            &mut bytes,
            ProtocolMsg::Payload(Payload {
                id: BytesMut::from(vec![0, 1, 2, 3, 4, 5]),
                msg: BytesMut::from(vec![255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245]),
            }),
        );

        // Ensure the codec data is correct
        assert_eq!(bytes.len(), 2);
        assert_eq!(codec.id_len, 0);
        assert_eq!(codec.msg_len, 0);

        bytes.reserve(4);
        bytes.put_slice(&[0, 0, 0, 5]); // Write msg len
        let res = codec.decode(&mut bytes).unwrap();
        assert!(res.is_none());

        assert_eq!(bytes.len(), 0);
        assert_eq!(codec.id_len, 3);
        assert_eq!(codec.msg_len, 5);

        bytes.reserve(8);
        bytes.put_slice(&vec![
            1, 1, 1, // ID
            2, 2, 2, 2, 2, // Msg
        ]);
        assert_decode(
            &mut codec,
            &mut bytes,
            ProtocolMsg::Payload(Payload {
                id: BytesMut::from(vec![1, 1, 1]),
                msg: BytesMut::from(vec![2, 2, 2, 2, 2]),
            }),
        );
    }

    #[test]
    fn test_decode_multiple_full_frames() {
        let mut codec = Codec::new();
        #[rustfmt::skip]
        let mut bytes = BytesMut::from(vec![
            // First payload
            1, // Message type
            6, // ID len
            0, 0, 0, 11, // Msg len
            0, 1, 2, 3, 4, 5, // ID
            255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245, // Msg
            // Second payload
            1,
            3,
            0, 0, 0, 4,
            1, 1, 1,
            2, 2, 2, 2,
            // Third payload
            1,
            4,
            0, 0, 0, 5,
            3, 3, 3, 3,
            4, 4, 4, 4, 4
        ]);

        assert_decode(
            &mut codec,
            &mut bytes,
            ProtocolMsg::Payload(Payload {
                id: BytesMut::from(vec![0, 1, 2, 3, 4, 5]),
                msg: BytesMut::from(vec![255, 254, 253, 252, 251, 250, 249, 248, 247, 246, 245]),
            }),
        );

        assert_decode(
            &mut codec,
            &mut bytes,
            ProtocolMsg::Payload(Payload {
                id: BytesMut::from(vec![1, 1, 1]),
                msg: BytesMut::from(vec![2, 2, 2, 2]),
            }),
        );

        assert_decode(
            &mut codec,
            &mut bytes,
            ProtocolMsg::Payload(Payload {
                id: BytesMut::from(vec![3, 3, 3, 3]),
                msg: BytesMut::from(vec![4, 4, 4, 4, 4]),
            }),
        );

        assert_eq!(bytes.len(), 0);
        assert_eq!(codec.id_len, 0);
        assert_eq!(codec.msg_len, 0);
    }
}
