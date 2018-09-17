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
        let mut payload = Vec::<u8>::with_capacity(10240);
        payload.push_u32(pl.id);
        if let Some(msg) = pl.msg {
            match msg {
                RpcMsg::Handshake(hs) => {
                    payload.push(RpcMsgType::HANDSHAKE as u8);
                    payload.push(hs.peer_type as u8);
                },
                RpcMsg::Properties(props) => {
                    payload.push(RpcMsgType::PROPERTIES as u8);
                    if let Some(props) = props {
                        payload.push_u64(props.height);
                    }
                },
                RpcMsg::Event(evt) => {
                    payload.push(RpcMsgType::EVENT as u8);
                    match evt {
                        RpcEvent::Tx(tx) => {
                            payload.push(RpcEventType::TX as u8);
                            if let Some(tx) = tx {
                                let mut v = Vec::with_capacity(4096);
                                tx.encode_with_sigs(&mut v);
                                payload.extend_from_slice(&v);
                            }
                        },
                        RpcEvent::Block(block) => {
                            payload.push(RpcEventType::BLOCK as u8);
                            if let Some(block) = block {
                                let mut v = Vec::with_capacity(10240);
                                block.encode_with_tx(&mut v);
                                payload.extend_from_slice(&v);
                            }
                        }
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
            let split = buf.split_to(msg_len as usize);
            let mut cur = Cursor::new(split.as_ref());
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
                    let peer_type = match cur.get_u8() {
                        t if t == PeerType::NODE as u8 => PeerType::NODE,
                        t if t == PeerType::WALLET as u8 => PeerType::WALLET,
                        _ => return Err(Error::new(ErrorKind::Other, "invalid peer type"))
                    };
                    RpcMsg::Handshake(RpcMsgHandshake {
                        peer_type
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
                t if t == RpcMsgType::EVENT as u8 => {
                    let event_type = cur.get_u8();
                    match event_type {
                        t if t == RpcEventType::TX as u8 => {
                            if u64::from(msg_len) - cur.position() > 0 {
                                let tx = TxVariant::decode_with_sigs(&mut cur);
                                if let Some(tx) = tx {
                                    RpcMsg::Event(RpcEvent::Tx(Some(tx)))
                                } else {
                                    return Err(Error::new(ErrorKind::Other, "failed to decode tx"))
                                }
                            } else {
                                RpcMsg::Event(RpcEvent::Tx(None))
                            }
                        },
                        t if t == RpcEventType::BLOCK as u8 => {
                            if u64::from(msg_len) - cur.position() > 0 {
                                let block = SignedBlock::decode_with_tx(&mut cur);
                                if let Some(block) = block {
                                    RpcMsg::Event(RpcEvent::Block(Some(block)))
                                } else {
                                    return Err(Error::new(ErrorKind::Other, "failed to decode signed block"))
                                }
                            } else {
                                RpcMsg::Event(RpcEvent::Tx(None))
                            }
                        },
                        _ => return Err(Error::new(ErrorKind::Other, "invalid event type"))
                    }
                }
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
