use crate::{
    script::{EvalErr, EvalErrType, InitErr},
    serializer::*,
};
use std::io::{self, Cursor};

#[derive(Copy, Clone, Debug)]
pub struct Config {
    pub skip_reward: bool,
}

impl Config {
    pub const fn strict() -> Self {
        Self { skip_reward: false }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockError {
    InvalidBlockHeight,
    InvalidMerkleRoot,
    InvalidSignature,
    InvalidPrevHash,
    InvalidHash,
    Tx(TxErr),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TxErr {
    ScriptTooLarge,
    ScriptEval(EvalErr),
    ScriptHashMismatch,
    ScriptRetFalse,
    Arithmetic,
    InsufficientBalance,
    InsufficientFeeAmount,
    TooManySignatures,
    TxProhibited,
    TxExpired,
    TxDupe,
}

impl TxErr {
    pub fn serialize(self, buf: &mut Vec<u8>) {
        match self {
            TxErr::ScriptTooLarge => buf.push(0),
            TxErr::ScriptEval(err) => {
                buf.push(1);
                buf.push_u32(err.pos);
                buf.push(err.err as u8);
            }
            TxErr::ScriptHashMismatch => buf.push(2),
            TxErr::ScriptRetFalse => buf.push(3),
            TxErr::Arithmetic => buf.push(4),
            TxErr::InsufficientBalance => buf.push(5),
            TxErr::InsufficientFeeAmount => buf.push(6),
            TxErr::TooManySignatures => buf.push(7),
            TxErr::TxProhibited => buf.push(8),
            TxErr::TxExpired => buf.push(9),
            TxErr::TxDupe => buf.push(10),
        }
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tag = cursor.take_u8()?;
        Ok(match tag {
            0 => TxErr::ScriptTooLarge,
            1 => {
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
            2 => TxErr::ScriptHashMismatch,
            3 => TxErr::ScriptRetFalse,
            4 => TxErr::Arithmetic,
            5 => TxErr::InsufficientBalance,
            6 => TxErr::InsufficientFeeAmount,
            7 => TxErr::TooManySignatures,
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

impl From<InitErr> for TxErr {
    fn from(err: InitErr) -> Self {
        match err {
            InitErr::ScriptTooLarge => TxErr::ScriptTooLarge,
            InitErr::TooManySignatures => TxErr::TooManySignatures,
        }
    }
}
