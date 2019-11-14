use crate::{
    script::{EvalErr, EvalErrType},
    serializer::*,
};
use std::io::{self, Cursor};

pub type SkipFlags = u8;

#[allow(clippy::identity_op)]
pub const SKIP_NONE: u8 = 1 << 0;
pub const SKIP_REWARD_TX: u8 = 1 << 1;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockErr {
    InvalidBlockHeight,
    InvalidMerkleRoot,
    InvalidSignature,
    InvalidPrevHash,
    InvalidHash,
    Tx(TxErr),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TxErr {
    ScriptEval(EvalErr),
    ScriptHashMismatch,
    ScriptRetFalse,
    Arithmetic,
    InsufficientBalance,
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
            TxErr::ScriptRetFalse => buf.push(0x02),
            TxErr::Arithmetic => buf.push(0x03),
            TxErr::InsufficientBalance => buf.push(0x04),
            TxErr::InvalidFeeAmount => buf.push(0x05),
            TxErr::TooManySignatures => buf.push(0x06),
            TxErr::TxTooLarge => buf.push(0x07),
            TxErr::TxProhibited => buf.push(0x08),
            TxErr::TxExpired => buf.push(0x09),
            TxErr::TxDupe => buf.push(0x0A),
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        Ok(match tag {
            0x00 => {
                let pos = cursor.take_u32()?;
                let kind = match cursor.take_u8()? {
                    t if t == EvalErrType::UnexpectedEOF as u8 => EvalErrType::UnexpectedEOF,
                    t if t == EvalErrType::UnknownOp as u8 => EvalErrType::UnknownOp,
                    t if t == EvalErrType::InvalidItemOnStack as u8 => {
                        EvalErrType::InvalidItemOnStack
                    }
                    t if t == EvalErrType::StackOverflow as u8 => EvalErrType::StackOverflow,
                    t if t == EvalErrType::StackUnderflow as u8 => EvalErrType::StackUnderflow,
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
            0x02 => TxErr::ScriptRetFalse,
            0x03 => TxErr::Arithmetic,
            0x04 => TxErr::InsufficientBalance,
            0x05 => TxErr::InvalidFeeAmount,
            0x06 => TxErr::TooManySignatures,
            0x07 => TxErr::TxTooLarge,
            0x08 => TxErr::TxProhibited,
            0x09 => TxErr::TxExpired,
            0x0A => TxErr::TxDupe,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize TxErr",
                ))
            }
        })
    }
}
