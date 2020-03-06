use crate::{
    script::{EvalErr, EvalErrType},
    serializer::*,
};
use std::{
    convert::TryFrom,
    io::{self, Cursor},
};

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockErr {
    InvalidBlockHeight,
    InvalidReceiptRoot,
    InvalidSignature,
    InvalidPrevHash,
    Tx(TxErr),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TxErr {
    ScriptEval(EvalErr),
    ScriptHashMismatch,
    AccountNotFound,
    AccountAlreadyExists,
    InvalidAccountPermissions,
    Arithmetic,
    InvalidAmount,
    InvalidFeeAmount,
    TooManySignatures,
    TxTooLarge,
    TxProhibited,
    TxExpired,
    TxDupe,
}

impl TxErr {
    pub fn serialize(self, buf: &mut Vec<u8>) {
        match self {
            TxErr::ScriptEval(err) => {
                buf.push(0x00);
                buf.push_u32(err.pos);
                buf.push(err.err as u8);
            }
            TxErr::ScriptHashMismatch => buf.push(0x01),
            TxErr::AccountNotFound => buf.push(0x02),
            TxErr::AccountAlreadyExists => buf.push(0x03),
            TxErr::InvalidAccountPermissions => buf.push(0x04),
            TxErr::Arithmetic => buf.push(0x05),
            TxErr::InvalidAmount => buf.push(0x06),
            TxErr::InvalidFeeAmount => buf.push(0x07),
            TxErr::TooManySignatures => buf.push(0x08),
            TxErr::TxTooLarge => buf.push(0x09),
            TxErr::TxProhibited => buf.push(0x0A),
            TxErr::TxExpired => buf.push(0x0B),
            TxErr::TxDupe => buf.push(0x0C),
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        Ok(match tag {
            0x00 => {
                let pos = cursor.take_u32()?;
                let kind = EvalErrType::try_from(cursor.take_u8()?).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "failed to deserialize EvalErrType",
                    )
                })?;
                TxErr::ScriptEval(EvalErr::new(pos, kind))
            }
            0x01 => TxErr::ScriptHashMismatch,
            0x02 => TxErr::AccountNotFound,
            0x03 => TxErr::AccountAlreadyExists,
            0x04 => TxErr::InvalidAccountPermissions,
            0x05 => TxErr::Arithmetic,
            0x06 => TxErr::InvalidAmount,
            0x07 => TxErr::InvalidFeeAmount,
            0x08 => TxErr::TooManySignatures,
            0x09 => TxErr::TxTooLarge,
            0x0A => TxErr::TxProhibited,
            0x0B => TxErr::TxExpired,
            0x0C => TxErr::TxDupe,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize TxErr",
                ))
            }
        })
    }
}
