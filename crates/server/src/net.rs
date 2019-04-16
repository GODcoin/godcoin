use godcoin::{serializer::*, prelude::SignedBlock};
use std::io::{self, Cursor, Error};
use actix_web::HttpResponse;

#[repr(u8)]
pub enum MsgType {
    Error = 0,
    GetBlock = 1,
}

pub enum MsgRequest {
    GetBlock(u64), // height
}

pub fn deserialize_req(bytes: bytes::Bytes) -> Result<MsgRequest, Error> {
    let mut cursor = Cursor::<&[u8]>::new(&bytes);

    let tag = cursor.take_u8()?;
    match tag {
        t if t == MsgType::GetBlock as u8 => {
            let height = cursor.take_u64()?;
            Ok(MsgRequest::GetBlock(height))
        },
        _ => Err(Error::new(io::ErrorKind::InvalidInput, "invalid msg request"))
    }
}

#[repr(u16)]
pub enum ErrorKind {
    UnknownError = 0,
    InvalidHeight = 1,
}

pub enum MsgResponse {
    Error(ErrorKind, Option<String>), // code, message
    GetBlock(SignedBlock),
}

impl Into<HttpResponse> for MsgResponse {
    fn into(self) -> HttpResponse {
        match self {
            MsgResponse::Error(code, msg) => {
                match msg {
                    Some(msg) => {
                        let mut buf = Vec::with_capacity(3 + msg.len());
                        buf.push(MsgType::Error as u8);
                        buf.push_u16(code as u16);
                        buf.push_bytes(msg.as_bytes());
                        HttpResponse::Ok().body(buf)
                    },
                    None => {
                        let mut buf = Vec::with_capacity(3);
                        buf.push(MsgType::Error as u8);
                        buf.push_u16(code as u16);
                        HttpResponse::Ok().body(buf)
                    }
                }
            }
            MsgResponse::GetBlock(block) => {
                let mut buf = Vec::with_capacity(1_048_576);
                buf.push(MsgType::GetBlock as u8);
                block.encode_with_tx(&mut buf);
                HttpResponse::Ok().body(buf)
            }
        }
    }
}
