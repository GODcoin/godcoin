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
    UnknownOp = 0x02,
    InvalidItemOnStack = 0x03,
    StackOverflow = 0x04,
    StackUnderflow = 0x05,
    Arithmetic = 0x06,
}

impl TryFrom<u8> for EvalErrType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
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
                return Err(())
            }
        })
    }
}
