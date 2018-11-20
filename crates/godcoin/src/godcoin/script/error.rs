#[derive(Debug, Clone)]
pub struct EvalErr {
    pub pos: u32,
    pub err: EvalErrType
}

impl EvalErr {
    pub fn new(pos: usize, err: EvalErrType) -> EvalErr {
        EvalErr { pos: pos as u32, err }
    }
}

#[repr(u8)]
#[derive(PartialEq, Debug, Clone)]
pub enum EvalErrType {
    UnexpectedEOF,
    UnknownOp,
    InvalidItemOnStack,
    StackOverflow,
    StackUnderflow,
}
