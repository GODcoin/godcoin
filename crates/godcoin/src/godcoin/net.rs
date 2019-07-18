use crate::{
    prelude::{verify::TxErr, *},
    serializer::*,
};
use std::io::{self, Cursor, Error};

#[derive(Clone, Debug, PartialEq)]
pub enum RequestType {
    Batch(Vec<MsgRequest>),
    Single(MsgRequest),
}

impl RequestType {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            RequestType::Batch(batch) => {
                buf.push(0);
                buf.push_u32(batch.len() as u32);
                for req in batch {
                    req.serialize(buf);
                }
            }
            RequestType::Single(req) => {
                buf.push(1);
                req.serialize(buf);
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            0 => {
                let len = cursor.take_u32()?;
                let mut batch = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    batch.push(MsgRequest::deserialize(cursor)?);
                }
                Ok(RequestType::Batch(batch))
            }
            1 => Ok(RequestType::Single(MsgRequest::deserialize(cursor)?)),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize type",
                ))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResponseType {
    Batch(Vec<MsgResponse>),
    Single(MsgResponse),
}

impl ResponseType {
    pub fn unwrap_batch(self) -> Vec<MsgResponse> {
        match self {
            ResponseType::Batch(batch) => batch,
            _ => panic!("expected batch response type"),
        }
    }

    pub fn unwrap_single(self) -> MsgResponse {
        match self {
            ResponseType::Single(res) => res,
            _ => panic!("expected single response type"),
        }
    }

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            ResponseType::Batch(batch) => {
                buf.push(0);
                buf.push_u32(batch.len() as u32);
                for res in batch {
                    res.serialize(buf);
                }
            }
            ResponseType::Single(res) => {
                buf.push(1);
                res.serialize(buf);
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            0 => {
                let len = cursor.take_u32()?;
                let mut batch = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    batch.push(MsgResponse::deserialize(cursor)?);
                }
                Ok(ResponseType::Batch(batch))
            }
            1 => Ok(ResponseType::Single(MsgResponse::deserialize(cursor)?)),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize type",
                ))
            }
        }
    }
}

#[repr(u8)]
pub enum MsgType {
    Error = 0,
    Broadcast = 10,
    GetProperties = 20,
    GetBlock = 21,
    GetAddressInfo = 22,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MsgRequest {
    Broadcast(TxVariant),
    GetProperties,
    GetBlock(u64), // height
    GetAddressInfo(ScriptHash),
}

impl MsgRequest {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            MsgRequest::Broadcast(tx) => {
                buf.reserve_exact(4096);
                buf.push(MsgType::Broadcast as u8);
                tx.serialize(buf);
            }
            MsgRequest::GetProperties => buf.push(MsgType::GetProperties as u8),
            MsgRequest::GetBlock(height) => {
                buf.reserve_exact(9);
                buf.push(MsgType::GetBlock as u8);
                buf.push_u64(*height);
            }
            MsgRequest::GetAddressInfo(addr) => {
                buf.reserve_exact(33);
                buf.push(MsgType::GetAddressInfo as u8);
                buf.push_script_hash(addr);
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            t if t == MsgType::Broadcast as u8 => {
                let tx = TxVariant::deserialize(cursor)
                    .ok_or_else(|| Error::new(io::ErrorKind::InvalidData, "failed to decode tx"))?;
                Ok(MsgRequest::Broadcast(tx))
            }
            t if t == MsgType::GetProperties as u8 => Ok(MsgRequest::GetProperties),
            t if t == MsgType::GetBlock as u8 => {
                let height = cursor.take_u64()?;
                Ok(MsgRequest::GetBlock(height))
            }
            t if t == MsgType::GetAddressInfo as u8 => {
                let addr = cursor.take_script_hash()?;
                Ok(MsgRequest::GetAddressInfo(addr))
            }
            _ => Err(Error::new(
                io::ErrorKind::InvalidData,
                "invalid msg request",
            )),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ErrorKind {
    Io,
    BytesRemaining,
    InvalidHeight,
    TxValidation(TxErr),
}

impl ErrorKind {
    fn serialize(self, buf: &mut Vec<u8>) {
        match self {
            ErrorKind::Io => buf.push(0),
            ErrorKind::BytesRemaining => buf.push(1),
            ErrorKind::InvalidHeight => buf.push(2),
            ErrorKind::TxValidation(err) => {
                buf.push(3);
                err.serialize(buf);
            }
        }
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        Ok(match tag {
            0 => ErrorKind::Io,
            1 => ErrorKind::BytesRemaining,
            2 => ErrorKind::InvalidHeight,
            3 => ErrorKind::TxValidation(TxErr::deserialize(cursor)?),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize ErrorKind",
                ))
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MsgResponse {
    Error(ErrorKind),
    Broadcast,
    GetProperties(Properties),
    GetBlock(SignedBlock),
    GetAddressInfo(AddressInfo),
}

impl MsgResponse {
    pub fn is_err(&self) -> bool {
        match self {
            MsgResponse::Error(..) => true,
            _ => false,
        }
    }

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        use std::mem;

        match self {
            MsgResponse::Error(err) => {
                buf.reserve_exact(2048);
                buf.push(MsgType::Error as u8);
                err.serialize(buf);
            }
            MsgResponse::Broadcast => buf.push(MsgType::Broadcast as u8),
            MsgResponse::GetProperties(props) => {
                buf.reserve_exact(4096 + mem::size_of::<Properties>());
                buf.push(MsgType::GetProperties as u8);
                buf.push_u64(props.height);
                {
                    let mut tx_buf = Vec::with_capacity(4096);
                    props.owner.serialize(&mut tx_buf);
                    buf.extend_from_slice(&tx_buf);
                }
                buf.push_asset(props.network_fee);
                buf.push_asset(props.token_supply);
            }
            MsgResponse::GetBlock(block) => {
                buf.reserve_exact(1_048_576);
                buf.push(MsgType::GetBlock as u8);
                block.serialize_with_tx(buf);
            }
            MsgResponse::GetAddressInfo(info) => {
                buf.reserve_exact(1 + (mem::size_of::<Asset>() * 3));
                buf.push(MsgType::GetAddressInfo as u8);
                buf.push_asset(info.net_fee);
                buf.push_asset(info.addr_fee);
                buf.push_asset(info.balance);
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            t if t == MsgType::Error as u8 => {
                let err = ErrorKind::deserialize(cursor)?;
                Ok(MsgResponse::Error(err))
            }
            t if t == MsgType::Broadcast as u8 => Ok(MsgResponse::Broadcast),
            t if t == MsgType::GetProperties as u8 => {
                let height = cursor.take_u64()?;
                let owner = {
                    let tx = TxVariant::deserialize(cursor).ok_or_else(|| {
                        Error::new(io::ErrorKind::InvalidData, "failed to deserialize owner tx")
                    })?;
                    match tx {
                        TxVariant::V0(ref var) => match var {
                            TxVariantV0::OwnerTx(_) => Box::new(tx),
                            _ => {
                                return Err(Error::new(
                                    io::ErrorKind::InvalidData,
                                    "expected owner tx",
                                ))
                            }
                        },
                    }
                };
                let network_fee = cursor.take_asset()?;
                let token_supply = cursor.take_asset()?;
                Ok(MsgResponse::GetProperties(Properties {
                    height,
                    owner,
                    network_fee,
                    token_supply,
                }))
            }
            t if t == MsgType::GetBlock as u8 => {
                let block = SignedBlock::deserialize_with_tx(cursor)
                    .ok_or_else(|| Error::from(io::ErrorKind::UnexpectedEof))?;
                Ok(MsgResponse::GetBlock(block))
            }
            t if t == MsgType::GetAddressInfo as u8 => {
                let net_fee = cursor.take_asset()?;
                let addr_fee = cursor.take_asset()?;
                let balance = cursor.take_asset()?;
                Ok(MsgResponse::GetAddressInfo(AddressInfo {
                    net_fee,
                    addr_fee,
                    balance,
                }))
            }
            _ => Err(Error::new(
                io::ErrorKind::InvalidData,
                "invalid msg response",
            )),
        }
    }
}
