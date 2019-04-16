use crate::{prelude::SignedBlock, serializer::*};
use std::convert::{TryFrom, TryInto};
use std::io::{self, Cursor, Error, Read};

#[repr(u8)]
pub enum MsgType {
    Error = 0,
    GetBlock = 1,
}

pub enum MsgRequest {
    GetBlock(u64), // height
}

impl MsgRequest {
    pub fn serialize(&self, bytes: &mut Vec<u8>) {
        match self {
            MsgRequest::GetBlock(height) => {
                bytes.push(MsgType::GetBlock as u8);
                bytes.push_u64(*height);
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            t if t == MsgType::GetBlock as u8 => {
                let height = cursor.take_u64()?;
                Ok(MsgRequest::GetBlock(height))
            }
            _ => Err(Error::new(
                io::ErrorKind::InvalidData,
                "invalid msg request",
            )),
        }
    }
}

#[repr(u16)]
pub enum ErrorKind {
    UnknownError = 0,
    InvalidHeight = 1,
}

impl TryFrom<u16> for ErrorKind {
    type Error = Error;

    fn try_from(value: u16) -> Result<ErrorKind, Error> {
        match value {
            e if e == ErrorKind::UnknownError as u16 => Ok(ErrorKind::UnknownError),
            e if e == ErrorKind::InvalidHeight as u16 => Ok(ErrorKind::InvalidHeight),
            _ => Err(Error::new(io::ErrorKind::InvalidData, "unknown error kind")),
        }
    }
}

pub enum MsgResponse {
    Error(ErrorKind, Option<String>), // code, message
    GetBlock(SignedBlock),
}

impl MsgResponse {
    pub fn serialize(self) -> Vec<u8> {
        match self {
            MsgResponse::Error(code, msg) => match msg {
                Some(msg) => {
                    let mut buf = Vec::with_capacity(3 + msg.len());
                    buf.push(MsgType::Error as u8);
                    buf.push_u16(code as u16);
                    buf.push_bytes(msg.as_bytes());
                    buf
                }
                None => {
                    let mut buf = Vec::with_capacity(7);
                    buf.push(MsgType::Error as u8);
                    buf.push_u16(code as u16);
                    buf.push_bytes(&[]);
                    buf
                }
            },
            MsgResponse::GetBlock(block) => {
                let mut buf = Vec::with_capacity(1_048_576);
                buf.push(MsgType::GetBlock as u8);
                block.encode_with_tx(&mut buf);
                buf
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        match tag {
            t if t == MsgType::Error as u8 => {
                let kind = cursor.take_u16()?.try_into()?;
                let msg = {
                    let mut buf = Vec::with_capacity(1024);
                    let read = cursor.read_to_end(&mut buf)?;
                    if read > 0 {
                        Some(String::from_utf8_lossy(&buf).into_owned())
                    } else {
                        None
                    }
                };
                Ok(MsgResponse::Error(kind, msg))
            }
            t if t == MsgType::GetBlock as u8 => {
                let block = SignedBlock::decode_with_tx(cursor)
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
