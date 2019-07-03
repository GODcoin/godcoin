use crate::{
    script::{EvalErr, EvalErrType},
    serializer::*,
};
use std::io::{self, Cursor};

pub type SkipFlags = u8;

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
                buf.push(0);
                buf.push_u32(err.pos);
                buf.push(err.err as u8);
            }
            TxErr::ScriptHashMismatch => buf.push(1),
            TxErr::ScriptRetFalse => buf.push(2),
            TxErr::Arithmetic => buf.push(3),
            TxErr::InsufficientBalance => buf.push(4),
            TxErr::InvalidFeeAmount => buf.push(5),
            TxErr::TooManySignatures => buf.push(6),
            TxErr::TxTooLarge => buf.push(7),
            TxErr::TxProhibited => buf.push(8),
            TxErr::TxExpired => buf.push(9),
            TxErr::TxDupe => buf.push(10),
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        Ok(match tag {
            0 => {
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
            1 => TxErr::ScriptHashMismatch,
            2 => TxErr::ScriptRetFalse,
            3 => TxErr::Arithmetic,
            4 => TxErr::InsufficientBalance,
            5 => TxErr::InvalidFeeAmount,
            6 => TxErr::TooManySignatures,
            7 => TxErr::TxTooLarge,
            8 => TxErr::TxProhibited,
            9 => TxErr::TxExpired,
            10 => TxErr::TxDupe,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "failed to deserialize TxErr",
                ))
            }
        })
    }
}
