use crate::{
    prelude::{verify::TxErr, Properties, SignedBlock, TxVariant},
    serializer::*,
};
use std::io::{self, Cursor, Error};

#[repr(u8)]
pub enum MsgType {
    Error = 0,
    GetProperties = 1,
    GetBlock = 2,
    Broadcast = 3,
}

pub enum MsgRequest {
    GetProperties,
    GetBlock(u64), // height
    Broadcast(TxVariant),
}

impl MsgRequest {
    pub fn serialize(self) -> Vec<u8> {
        match self {
            MsgRequest::GetProperties => vec![MsgType::GetProperties as u8],
            MsgRequest::GetBlock(height) => {
                let mut buf = Vec::with_capacity(9);
                buf.push(MsgType::GetBlock as u8);
                buf.push_u64(height);
                buf
            }
            MsgRequest::Broadcast(tx) => {
                let mut buf = Vec::with_capacity(4096);
                buf.push(MsgType::Broadcast as u8);
                tx.serialize(&mut buf);
                buf
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            t if t == MsgType::GetProperties as u8 => Ok(MsgRequest::GetProperties),
            t if t == MsgType::GetBlock as u8 => {
                let height = cursor.take_u64()?;
                Ok(MsgRequest::GetBlock(height))
            }
            t if t == MsgType::Broadcast as u8 => {
                let tx = TxVariant::deserialize(cursor)
                    .ok_or_else(|| Error::new(io::ErrorKind::InvalidData, "failed to decode tx"))?;
                Ok(MsgRequest::Broadcast(tx))
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
    InvalidHeight,
    TxValidation(TxErr),
}

impl ErrorKind {
    fn serialize(self, buf: &mut Vec<u8>) {
        match self {
            ErrorKind::Io => buf.push(0),
            ErrorKind::InvalidHeight => buf.push(1),
            ErrorKind::TxValidation(err) => {
                buf.push(2);
                err.serialize(buf);
            }
        }
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        Ok(match tag {
            0 => ErrorKind::Io,
            1 => ErrorKind::InvalidHeight,
            2 => ErrorKind::TxValidation(TxErr::deserialize(cursor)?),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize ErrorKind",
                ))
            }
        })
    }
}

#[derive(Clone, Debug)]
pub enum MsgResponse {
    Error(ErrorKind),
    GetProperties(Properties),
    GetBlock(SignedBlock),
    Broadcast(),
}

impl MsgResponse {
    pub fn is_err(&self) -> bool {
        match self {
            MsgResponse::Error(..) => true,
            _ => false,
        }
    }

    pub fn serialize(self) -> Vec<u8> {
        use std::mem;

        match self {
            MsgResponse::Error(err) => {
                let mut buf = Vec::with_capacity(2048);
                buf.push(MsgType::Error as u8);
                err.serialize(&mut buf);
                buf
            }
            MsgResponse::GetProperties(props) => {
                let mut buf = Vec::with_capacity(4096 + mem::size_of::<Properties>());
                buf.push(MsgType::GetProperties as u8);
                buf.push_u64(props.height);
                {
                    let mut tx_buf = Vec::with_capacity(4096);
                    TxVariant::OwnerTx(*props.owner).serialize(&mut tx_buf);
                    buf.extend_from_slice(&tx_buf);
                }
                buf.push_asset(props.network_fee);
                buf.push_asset(props.token_supply);
                buf
            }
            MsgResponse::GetBlock(block) => {
                let mut buf = Vec::with_capacity(1_048_576);
                buf.push(MsgType::GetBlock as u8);
                block.serialize_with_tx(&mut buf);
                buf
            }
            MsgResponse::Broadcast() => vec![MsgType::Broadcast as u8],
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            t if t == MsgType::Error as u8 => {
                let err = ErrorKind::deserialize(cursor)?;
                Ok(MsgResponse::Error(err))
            }
            t if t == MsgType::GetProperties as u8 => {
                let height = cursor.take_u64()?;
                let owner = {
                    let var = TxVariant::deserialize(cursor).ok_or_else(|| {
                        Error::new(io::ErrorKind::InvalidData, "failed to deserialize owner tx")
                    })?;
                    match var {
                        TxVariant::OwnerTx(tx) => Box::new(tx),
                        _ => {
                            return Err(Error::new(io::ErrorKind::InvalidData, "expected owner tx"))
                        }
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
            _ => Err(Error::new(
                io::ErrorKind::InvalidData,
                "invalid msg response",
            )),
        }
    }
}
