use std::convert::TryFrom;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct EvalErr {
    pub pos: u32,
    pub err: EvalErrType,
}

impl EvalErr {
    pub fn new(pos: u32, err: EvalErrType) -> EvalErr {
        EvalErr { pos, err }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum EvalErrType {
    ScriptRetFalse = 0x00,
    UnexpectedEOF = 0x01,
    HeaderReadErr = 0x02,
    InvalidEntryPoint = 0x03,
    UnknownOp = 0x04,
    UnknownFn = 0x05,
    UnknownArgType = 0x06,
    ArgDeserialization = 0x07,
    InvalidItemOnStack = 0x08,
    StackOverflow = 0x09,
    StackUnderflow = 0x0A,
    Arithmetic = 0x0B,
    InvalidAmount = 0x0C,
    AccountNotFound = 0x0D,
}

impl TryFrom<u8> for EvalErrType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            t if t == Self::ScriptRetFalse as u8 => Self::ScriptRetFalse,
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
