use std::convert::TryFrom;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct EvalErr {
    pub pos: u32,
    pub err: EvalErrKind,
}

impl EvalErr {
    pub fn new(pos: u32, err: EvalErrKind) -> EvalErr {
        EvalErr { pos, err }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum EvalErrKind {
    ScriptRetFalse = 0x00,
    Aborted = 0x01,
    UnexpectedEOF = 0x02,
    HeaderReadErr = 0x03,
    InvalidEntryPoint = 0x04,
    UnknownOp = 0x05,
    UnknownFn = 0x06,
    UnknownArgType = 0x07,
    ArgDeserialization = 0x08,
    InvalidItemOnStack = 0x09,
    StackOverflow = 0x0A,
    StackUnderflow = 0x0B,
    Arithmetic = 0x0C,
    InvalidAmount = 0x0D,
    AccountNotFound = 0x0E,
}

impl TryFrom<u8> for EvalErrKind {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            t if t == Self::ScriptRetFalse as u8 => Self::ScriptRetFalse,
            t if t == Self::Aborted as u8 => Self::Aborted,
            t if t == Self::UnexpectedEOF as u8 => Self::UnexpectedEOF,
            t if t == Self::HeaderReadErr as u8 => Self::HeaderReadErr,
            t if t == Self::InvalidEntryPoint as u8 => Self::InvalidEntryPoint,
            t if t == Self::UnknownOp as u8 => Self::UnknownOp,
            t if t == Self::UnknownFn as u8 => Self::UnknownFn,
            t if t == Self::UnknownArgType as u8 => Self::UnknownArgType,
            t if t == Self::ArgDeserialization as u8 => Self::ArgDeserialization,
            t if t == Self::InvalidItemOnStack as u8 => Self::InvalidItemOnStack,
            t if t == Self::StackOverflow as u8 => Self::StackOverflow,
            t if t == Self::StackUnderflow as u8 => Self::StackUnderflow,
            t if t == Self::Arithmetic as u8 => Self::Arithmetic,
            t if t == Self::InvalidAmount as u8 => Self::InvalidAmount,
            t if t == Self::AccountNotFound as u8 => Self::AccountNotFound,
            _ => return Err(()),
        })
    }
}
