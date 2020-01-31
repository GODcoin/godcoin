use crate::{
    script::{EvalErr, EvalErrType},
    serializer::*,
};
use std::io::{self, Cursor};

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
            TxErr::Arithmetic => buf.push(0x02),
            TxErr::InvalidAmount => buf.push(0x03),
            TxErr::InvalidFeeAmount => buf.push(0x04),
            TxErr::TooManySignatures => buf.push(0x05),
            TxErr::TxTooLarge => buf.push(0x06),
            TxErr::TxProhibited => buf.push(0x07),
            TxErr::TxExpired => buf.push(0x08),
            TxErr::TxDupe => buf.push(0x09),
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        Ok(match tag {
            0x00 => {
                let pos = cursor.take_u32()?;
                let kind = match cursor.take_u8()? {
                    t if t == EvalErrType::ScriptRetFalse as u8 => EvalErrType::ScriptRetFalse,
                    t if t == EvalErrType::UnexpectedEOF as u8 => EvalErrType::UnexpectedEOF,
                    t if t == EvalErrType::UnknownOp as u8 => EvalErrType::UnknownOp,
                    t if t == EvalErrType::InvalidItemOnStack as u8 => {
                        EvalErrType::InvalidItemOnStack
                    }
                    t if t == EvalErrType::StackOverflow as u8 => EvalErrType::StackOverflow,
                    t if t == EvalErrType::StackUnderflow as u8 => EvalErrType::StackUnderflow,
                    t if t == EvalErrType::Arithmetic as u8 => EvalErrType::Arithmetic,
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "failed to deserialize EvalErrType",
                        ))
                    }
                };
                TxErr::ScriptEval(EvalErr::new(pos, kind))
            }
            0x01 => TxErr::ScriptHashMismatch,
            0x02 => TxErr::Arithmetic,
            0x03 => TxErr::InvalidAmount,
            0x04 => TxErr::InvalidFeeAmount,
            0x05 => TxErr::TooManySignatures,
            0x06 => TxErr::TxTooLarge,
            0x07 => TxErr::TxProhibited,
            0x08 => TxErr::TxExpired,
            0x09 => TxErr::TxDupe,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize TxErr",
                ))
            }
        })
    }
}
