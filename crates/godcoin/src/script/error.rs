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
