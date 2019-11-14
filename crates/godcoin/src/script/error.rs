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
    UnexpectedEOF = 0x00,
    UnknownOp = 0x01,
    InvalidItemOnStack = 0x02,
    StackOverflow = 0x03,
    StackUnderflow = 0x04,
}
