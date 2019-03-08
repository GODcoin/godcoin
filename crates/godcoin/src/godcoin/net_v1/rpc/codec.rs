use bytes::{BufMut, BytesMut};
use std::io::{Cursor, Error, ErrorKind};
use tokio::codec::{Decoder, Encoder};

use super::*;
use crate::blockchain::Properties;
use crate::serializer::*;
use crate::tx::TxVariant;

// 5 MiB limit
const MAX_PAYLOAD_LEN: u32 = 5_242_880;

#[derive(Default)]
pub struct RpcCodec {
    msg_len: u32,
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
                    payload.push(RpcMsgType::Error as u8);
                    payload.push_bytes(err.as_bytes());
                }
                RpcMsg::Event(evt) => {
                    payload.push(RpcMsgType::Event as u8);
                    match *evt {
                        RpcEvent::Tx(tx) => {
                            payload.push(RpcEventType::TX as u8);
                            tx.encode_with_sigs(&mut payload);
                        }
                        RpcEvent::Block(block) => {
                            payload.push(RpcEventType::BLOCK as u8);
                            block.encode_with_tx(&mut payload);
                        }
                    }
                }
                RpcMsg::Handshake(peer_type) => {
                    payload.push(RpcMsgType::Handshake as u8);
                    payload.push(peer_type as u8);
                }
                RpcMsg::Broadcast(tx) => {
                    payload.push(RpcMsgType::Broadcast as u8);
                    tx.encode_with_sigs(&mut payload);
                }
                RpcMsg::Properties(rpc) => {
                    payload.push(RpcMsgType::Properties as u8);
                    if let Some(props) = rpc.res() {
                        payload.push(RpcVariantType::Res as u8);
                        payload.push_u64(props.height);

                        payload.push_asset(&props.token_supply.gold);
                        payload.push_asset(&props.token_supply.silver);

                        payload.push_asset(&props.network_fee.gold);
                        payload.push_asset(&props.network_fee.silver);
                    } else {
                        payload.push(RpcVariantType::Req as u8);
                    }
                }
                RpcMsg::Block(rpc) => {
                    payload.push(RpcMsgType::Block as u8);
                    match *rpc {
                        RpcVariant::Req(height) => {
                            payload.push(RpcVariantType::Req as u8);
                            payload.push_u64(height);
                        }
                        RpcVariant::Res(block) => {
                            payload.push(RpcVariantType::Res as u8);
                            if let Some(block) = block {
                                block.encode_with_tx(&mut payload);
                            }
                        }
                    }
                }
                RpcMsg::Balance(rpc) => {
                    payload.push(RpcMsgType::Balance as u8);
                    match rpc {
                        RpcVariant::Req(hash) => {
                            payload.push(RpcVariantType::Req as u8);
                            payload.push_script_hash(&hash);
                        }
                        RpcVariant::Res(bal) => {
                            payload.push(RpcVariantType::Res as u8);
                            payload.push_asset(&bal.gold);
                            payload.push_asset(&bal.silver);
                        }
                    }
                }
                RpcMsg::TotalFee(rpc) => {
                    payload.push(RpcMsgType::TotalFee as u8);
                    match rpc {
                        RpcVariant::Req(hash) => {
                            payload.push(RpcVariantType::Req as u8);
                            payload.push_script_hash(&hash);
                        }
                        RpcVariant::Res(bal) => {
                            payload.push(RpcVariantType::Res as u8);
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
                return Err(Error::new(ErrorKind::Other, "payload must be >4 bytes"));
            } else if self.msg_len > MAX_PAYLOAD_LEN {
                return Err(Error::new(
                    ErrorKind::Other,
                    format!("payload must be <={} bytes", MAX_PAYLOAD_LEN),
                ));
            }
            self.msg_len -= 4;
        }
        if self.msg_len != 0 && buf.len() >= self.msg_len as usize {
            let msg_len = self.msg_len;
            let split = buf.split_to(msg_len as usize);
            let mut cur = Cursor::new(split.as_ref());
            self.msg_len = 0;

            let id = cur.take_u32()?;
            if msg_len == 4 {
                return Ok(Some(RpcPayload { id, msg: None }));
            }

            let msg = match cur.take_u8()? {
                t if t == RpcMsgType::Error as u8 => {
                    let bytes = cur.take_bytes()?;
                    RpcMsg::Error(String::from_utf8_lossy(&bytes).into_owned())
                }
                t if t == RpcMsgType::Event as u8 => {
                    let event_type = cur.take_u8()?;
                    match event_type {
                        t if t == RpcEventType::TX as u8 => {
                            let tx = TxVariant::decode_with_sigs(&mut cur).ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode tx")
                            })?;
                            RpcMsg::Event(Box::new(RpcEvent::Tx(tx)))
                        }
                        t if t == RpcEventType::BLOCK as u8 => {
                            let block = SignedBlock::decode_with_tx(&mut cur).ok_or_else(|| {
                                Error::new(ErrorKind::Other, "failed to decode signed block")
                            })?;
                            RpcMsg::Event(Box::new(RpcEvent::Block(block)))
                        }
                        _ => {
                            return Err(Error::new(ErrorKind::Other, "invalid event type"));
                        }
                    }
                }
                t if t == RpcMsgType::Handshake as u8 => {
                    let peer_type = match cur.take_u8()? {
                        t if t == PeerType::NODE as u8 => PeerType::NODE,
                        t if t == PeerType::WALLET as u8 => PeerType::WALLET,
                        _ => {
                            return Err(Error::new(ErrorKind::Other, "invalid peer type"));
                        }
                    };
                    RpcMsg::Handshake(peer_type)
                }
                t if t == RpcMsgType::Broadcast as u8 => {
                    let tx = TxVariant::decode_with_sigs(&mut cur).ok_or_else(|| {
                        Error::new(ErrorKind::Other, "failed to decode broadcast tx")
                    })?;
                    RpcMsg::Broadcast(tx)
                }
                t if t == RpcMsgType::Properties as u8 => {
                    let rpc = cur.take_u8()?;
                    match rpc {
                        t if t == RpcVariantType::Req as u8 => {
                            RpcMsg::Properties(RpcVariant::Req(()))
                        }
                        t if t == RpcVariantType::Res as u8 => {
                            let height = cur.take_u64()?;
                            let token_supply = {
                                let gold = cur.take_asset()?;
                                let silver = cur.take_asset()?;
                                Balance { gold, silver }
                            };
                            let network_fee = {
                                let gold = cur.take_asset()?;
                                let silver = cur.take_asset()?;
                                Balance { gold, silver }
                            };
                            RpcMsg::Properties(RpcVariant::Res(Properties {
                                height,
                                token_supply,
                                network_fee,
                            }))
                        }
                        _ => {
                            return Err(Error::new(ErrorKind::Other, "invalid rpc type"));
                        }
                    }
                }
                t if t == RpcMsgType::Block as u8 => {
                    let rpc = cur.take_u8()?;
                    match rpc {
                        t if t == RpcVariantType::Req as u8 => {
                            let height = cur.take_u64()?;
                            RpcMsg::Block(Box::new(RpcVariant::Req(height)))
                        }
                        t if t == RpcVariantType::Res as u8 => {
                            if u64::from(msg_len).saturating_sub(cur.position()) > 0 {
                                let block =
                                    SignedBlock::decode_with_tx(&mut cur).ok_or_else(|| {
                                        Error::new(ErrorKind::Other, "failed to decode block")
                                    })?;
                                RpcMsg::Block(Box::new(RpcVariant::Res(Some(block))))
                            } else {
                                RpcMsg::Block(Box::new(RpcVariant::Res(None)))
                            }
                        }
                        _ => {
                            return Err(Error::new(ErrorKind::Other, "invalid rpc type"));
                        }
                    }
                }
                t if t == RpcMsgType::Balance as u8 => {
                    let rpc = cur.take_u8()?;
                    match rpc {
                        t if t == RpcVariantType::Req as u8 => {
                            let hash = cur.take_script_hash()?;
                            RpcMsg::Balance(RpcVariant::Req(hash))
                        }
                        t if t == RpcVariantType::Res as u8 => {
                            let gold = cur.take_asset()?;
                            let silver = cur.take_asset()?;
                            RpcMsg::Balance(RpcVariant::Res(Balance { gold, silver }))
                        }
                        _ => {
                            return Err(Error::new(ErrorKind::Other, "invalid rpc type"));
                        }
                    }
                }
                t if t == RpcMsgType::TotalFee as u8 => {
                    let rpc = cur.take_u8()?;
                    match rpc {
                        t if t == RpcVariantType::Req as u8 => {
                            let hash = cur.take_script_hash()?;
                            RpcMsg::TotalFee(RpcVariant::Req(hash))
                        }
                        t if t == RpcVariantType::Res as u8 => {
                            let gold = cur.take_asset()?;
                            let silver = cur.take_asset()?;
                            RpcMsg::TotalFee(RpcVariant::Res(Balance { gold, silver }))
                        }
                        _ => {
                            return Err(Error::new(ErrorKind::Other, "invalid rpc type"));
                        }
                    }
                }
                _ => {
                    return Err(Error::new(ErrorKind::Other, "invalid msg type"));
                }
            };

            Ok(Some(RpcPayload { id, msg: Some(msg) }))
        } else {
            Ok(None)
        }
    }
}
