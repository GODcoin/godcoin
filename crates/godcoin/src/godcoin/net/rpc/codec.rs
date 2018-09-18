use std::io::{Cursor, Error, ErrorKind};
use tokio_codec::{Encoder, Decoder};
use bytes::{Buf, BufMut, BytesMut};
use std::mem::size_of;
use std::io::Read;
use serializer::*;

use blockchain::Properties;
use tx::TxVariant;
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
                RpcMsg::Error(err) => {
                    payload.push_u32(err.len() as u32);
                    payload.push_bytes(err.as_bytes());
                },
                RpcMsg::Event(evt) => {
                    payload.push(RpcMsgType::Event as u8);
                    match evt {
                        RpcEvent::Tx(tx) => {
                            payload.push(RpcEventType::TX as u8);
                            tx.encode_with_sigs(&mut payload);
                        },
                        RpcEvent::Block(block) => {
                            payload.push(RpcEventType::BLOCK as u8);
                            block.encode_with_tx(&mut payload);
                        }
                    }
                },
                RpcMsg::Handshake(peer_type) => {
                    payload.push(RpcMsgType::Handshake as u8);
                    payload.push(peer_type as u8);
                },
                RpcMsg::Broadcast(tx) => {
                    payload.push(RpcMsgType::Broadcast as u8);
                    tx.encode_with_sigs(&mut payload);
                },
                RpcMsg::Properties(props) => {
                    payload.push(RpcMsgType::Properties as u8);
                    if let Some(props) = props {
                        payload.push_u64(props.height);
                    }
                },
                RpcMsg::Block(txrx) => {
                    payload.push(RpcMsgType::Block as u8);
                    match txrx {
                        TxRx::Tx(height) => {
                            payload.push(TxRxType::Tx as u8);
                            payload.push_u64(height);
                        },
                        TxRx::Rx(block) => {
                            payload.push(TxRxType::Rx as u8);
                            block.encode_with_tx(&mut payload);
                        }
                    }
                },
                RpcMsg::Balance(txrx) => {
                    payload.push(RpcMsgType::Balance as u8);
                    match txrx {
                        TxRx::Tx(addr) => {
                            payload.push(TxRxType::Tx as u8);
                            payload.push_pub_key(&addr);
                        },
                        TxRx::Rx(bal) => {
                            payload.push(TxRxType::Rx as u8);
                            payload.push_asset(&bal.gold);
                            payload.push_asset(&bal.silver);
                        }
                    }
                },
                RpcMsg::TotalFee(txrx) => {
                    payload.push(RpcMsgType::TotalFee as u8);
                    match txrx {
                        TxRx::Tx(addr) => {
                            payload.push(TxRxType::Tx as u8);
                            payload.push_pub_key(&addr);
                        },
                        TxRx::Rx(bal) => {
                            payload.push(TxRxType::Rx as u8);
                            payload.push_asset(&bal.gold);
                            payload.push_asset(&bal.silver);
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
                t if t == RpcMsgType::Error as u8 => {
                    let len = cur.get_u32_be();
                    if len > MAX_PAYLOAD_LEN {
                        return Err(Error::new(ErrorKind::Other, "error string too large"))
                    }
                    let mut buf = Vec::with_capacity(len as usize);
                    cur.read_exact(&mut buf).map_err(|_| {
                        Error::new(ErrorKind::Other, "failed to read error string")
                    })?;
                    RpcMsg::Error(String::from_utf8_lossy(&buf).into_owned())
                },
                t if t == RpcMsgType::Event as u8 => {
                    let event_type = cur.get_u8();
                    match event_type {
                        t if t == RpcEventType::TX as u8 => {
                            let tx = TxVariant::decode_with_sigs(&mut cur).ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode tx")
                            })?;
                            RpcMsg::Event(RpcEvent::Tx(tx))
                        },
                        t if t == RpcEventType::BLOCK as u8 => {
                            let block = SignedBlock::decode_with_tx(&mut cur).ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode signed block")
                            })?;
                            RpcMsg::Event(RpcEvent::Block(block))
                        },
                        _ => return Err(Error::new(ErrorKind::Other, "invalid event type"))
                    }
                },
                t if t == RpcMsgType::Handshake as u8 => {
                    let peer_type = match cur.get_u8() {
                        t if t == PeerType::NODE as u8 => PeerType::NODE,
                        t if t == PeerType::WALLET as u8 => PeerType::WALLET,
                        _ => return Err(Error::new(ErrorKind::Other, "invalid peer type"))
                    };
                    RpcMsg::Handshake(peer_type)
                },
                t if t == RpcMsgType::Broadcast as u8 => {
                    let tx = TxVariant::decode_with_sigs(&mut cur).ok_or_else(|| {
                        Error::new(ErrorKind::Other, "failed to decode broadcast tx")
                    })?;
                    RpcMsg::Broadcast(tx)
                },
                t if t == RpcMsgType::Properties as u8 => {
                    if u64::from(msg_len) - cur.position() >= size_of::<u64>() as u64 {
                        let height = cur.get_u64_be();
                        RpcMsg::Properties(Some(Properties { height }))
                    } else {
                        RpcMsg::Properties(None)
                    }
                },
                t if t == RpcMsgType::Block as u8 => {
                    let txrx = cur.get_u8();
                    match txrx {
                        t if t == TxRxType::Tx as u8 => {
                            let height = cur.get_u64_be();
                            RpcMsg::Block(TxRx::Tx(height))
                        },
                        t if t == TxRxType::Rx as u8 => {
                            let block = SignedBlock::decode_with_tx(&mut cur).ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode block")
                            })?;
                            RpcMsg::Block(TxRx::Rx(block))
                        },
                        _ => return Err(Error::new(ErrorKind::Other, "invlaid txrx type"))
                    }
                },
                t if t == RpcMsgType::Balance as u8 => {
                    let txrx = cur.get_u8();
                    match txrx {
                        t if t == TxRxType::Tx as u8 => {
                            let addr = cur.take_pub_key().ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode public key")
                            })?;
                            RpcMsg::Balance(TxRx::Tx(addr))
                        },
                        t if t == TxRxType::Rx as u8 => {
                            let gold = cur.take_asset().ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode gold asset")
                            })?;
                            let silver = cur.take_asset().ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode silver asset")
                            })?;
                            RpcMsg::Balance(TxRx::Rx(Balance { gold, silver }))
                        },
                        _ => return Err(Error::new(ErrorKind::Other, "invlaid txrx type"))
                    }
                },
                t if t == RpcMsgType::TotalFee as u8 => {
                    let txrx = cur.get_u8();
                    match txrx {
                        t if t == TxRxType::Tx as u8 => {
                            let addr = cur.take_pub_key().ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode public key")
                            })?;
                            RpcMsg::TotalFee(TxRx::Tx(addr))
                        },
                        t if t == TxRxType::Rx as u8 => {
                            let gold = cur.take_asset().ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode gold asset")
                            })?;
                            let silver = cur.take_asset().ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode silver asset")
                            })?;
                            RpcMsg::TotalFee(TxRx::Rx(Balance { gold, silver }))
                        },
                        _ => return Err(Error::new(ErrorKind::Other, "invlaid txrx type"))
                    }
                },
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
