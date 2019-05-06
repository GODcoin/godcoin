#[derive(Clone, Debug)]
pub struct EvalErr {
    pub pos: u32,
    pub err: EvalErrType,
}

impl EvalErr {
    pub fn new(pos: usize, err: EvalErrType) -> EvalErr {
        EvalErr {
            pos: pos as u32,
            err,
        }
    }
}

#[repr(u8)]
#[derive(Clone, Debug, PartialEq)]
pub enum EvalErrType {
    UnexpectedEOF,
    UnknownOp,
    InvalidItemOnStack,
    StackOverflow,
    StackUnderflow,
}
