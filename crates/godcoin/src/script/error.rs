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
    InvalidItemOnStack = 0x06,
    StackOverflow = 0x07,
    StackUnderflow = 0x08,
    Arithmetic = 0x09,
}

impl TryFrom<u8> for EvalErrType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            t if t == EvalErrType::ScriptRetFalse as u8 => EvalErrType::ScriptRetFalse,
            t if t == EvalErrType::UnexpectedEOF as u8 => EvalErrType::UnexpectedEOF,
            t if t == EvalErrType::HeaderReadErr as u8 => EvalErrType::HeaderReadErr,
            t if t == EvalErrType::InvalidEntryPoint as u8 => EvalErrType::InvalidEntryPoint,
            t if t == EvalErrType::UnknownOp as u8 => EvalErrType::UnknownOp,
            t if t == EvalErrType::UnknownFn as u8 => EvalErrType::UnknownFn,
            t if t == EvalErrType::InvalidItemOnStack as u8 => EvalErrType::InvalidItemOnStack,
            t if t == EvalErrType::StackOverflow as u8 => EvalErrType::StackOverflow,
            t if t == EvalErrType::StackUnderflow as u8 => EvalErrType::StackUnderflow,
            t if t == EvalErrType::Arithmetic as u8 => EvalErrType::Arithmetic,
            _ => return Err(()),
        })
    }
}
