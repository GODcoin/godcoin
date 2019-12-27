pub mod rpc;

use crate::{prelude::verify::TxErr, serializer::*};
use std::io::{self, Cursor, Error};

#[derive(Clone, Debug, PartialEq)]
pub struct Msg {
    /// Max value is reserved for subscription updates or deserialization errors that occur during request processing.
    /// When a request is received with a reserved id, an IO error is returned regardless if the request is valid.
    pub id: u32,
    pub body: Body,
}

impl Msg {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.push_u32(self.id);
        match &self.body {
            Body::Error(e) => {
                buf.push(BodyType::Error as u8);
                e.serialize(buf);
            }
            Body::Request(req) => {
                buf.push(BodyType::Request as u8);
                req.serialize(buf);
            }
            Body::Response(res) => {
                buf.push(BodyType::Response as u8);
                res.serialize(buf);
            }
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let id = cursor.take_u32()?;
        let body = match cursor.take_u8()? {
            t if t == BodyType::Error as u8 => Body::Error(ErrorKind::deserialize(cursor)?),
            t if t == BodyType::Request as u8 => Body::Request(rpc::Request::deserialize(cursor)?),
            t if t == BodyType::Response as u8 => {
                Body::Response(rpc::Response::deserialize(cursor)?)
            }
            _ => return Err(Error::new(io::ErrorKind::InvalidData, "invalid msg type")),
        };
        Ok(Self { id, body })
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum BodyType {
    // Error message
    Error = 0x00,
    // RPC request
    Request = 0x01,
    // RPC response
    Response = 0x02,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Error(ErrorKind),
    Request(rpc::Request),
    Response(rpc::Response),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ErrorKind {
    Io,
    BytesRemaining,
    InvalidRequest,
    InvalidHeight,
    TxValidation(TxErr),
}

impl ErrorKind {
    fn serialize(self, buf: &mut Vec<u8>) {
        match self {
            Self::Io => buf.push(0x00),
            Self::BytesRemaining => buf.push(0x01),
            Self::InvalidRequest => buf.push(0x02),
            Self::InvalidHeight => buf.push(0x03),
            Self::TxValidation(err) => {
                buf.reserve_exact(2048);
                buf.push(0x04);
                err.serialize(buf);
            }
        }
    }

    fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        Ok(match tag {
            0x00 => Self::Io,
            0x01 => Self::BytesRemaining,
            0x02 => Self::InvalidRequest,
            0x03 => Self::InvalidHeight,
            0x04 => Self::TxValidation(TxErr::deserialize(cursor)?),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize ErrorKind",
                ))
            }
        })
    }
}
