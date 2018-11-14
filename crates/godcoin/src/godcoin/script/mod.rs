use sodiumoxide::crypto::sign;

use crate::crypto::PublicKey;

pub mod constants;
pub mod builder;
pub mod error;
pub mod op;

use self::constants::*;
use self::error::*;
use self::op::*;

pub struct ScriptEngine {
    byte_code: Vec<u8>,
    pos: usize,
    stack: Vec<OpFrame>
}

impl ScriptEngine {

    pub fn new(byte_code: Vec<u8>) -> Option<Self> {
        if byte_code.len() > MAX_BYTE_SIZE { return None }
        Some(Self {
            byte_code,
            pos: 0,
            stack: Vec::with_capacity(MAX_FRAME_STACK)
        })
    }

    pub fn eval(&mut self) -> Result<bool, EvalErr> {
        self.pos = 0;
        let mut if_marker = 0;
        let mut ignore_else = false;
        loop {
            let op = match self.consume_op()? {
                Some(op) => op,
                None => break
            };
            match op {
                // Push value
                OpFrame::False => {
                    self.insert(OpFrame::False)?;
                },
                OpFrame::True => {
                    self.insert(OpFrame::True)?;
                },
                OpFrame::PubKey(key) => {
                    self.insert(OpFrame::PubKey(key))?;
                },
                // Control
                OpFrame::OpIf => {
                    if_marker += 1;
                    ignore_else = self.pop_bool()?;
                    if ignore_else { continue; }
                    let req_if_marker = if_marker;
                    self.consume_op_until(|op| {
                        if op == OpFrame::OpIf {
                            if_marker += 1;
                            false
                        } else if op == OpFrame::OpElse {
                            if_marker == req_if_marker
                        } else if op == OpFrame::OpEndIf {
                            let do_break = if_marker == req_if_marker;
                            if_marker -= 1;
                            do_break
                        } else {
                            false
                        }
                    })?;
                },
                OpFrame::OpElse => {
                    if !ignore_else { continue; }
                    let req_if_marker = if_marker;
                    self.consume_op_until(|op| {
                        if op == OpFrame::OpIf {
                            if_marker += 1;
                            false
                        } else if op == OpFrame::OpElse {
                            if_marker == req_if_marker
                        } else if op == OpFrame::OpEndIf {
                            let do_break = if_marker == req_if_marker;
                            if_marker -= 1;
                            do_break
                        } else {
                            false
                        }
                    })?;
                },
                OpFrame::OpEndIf => {
                    if_marker -= 1;
                },
                OpFrame::OpReturn => {
                    if_marker = 0;
                    break;
                }
            }
        }

        if if_marker > 0 {
            return Err(self.new_err(EvalErrType::UnexpectedEOF))
        }

        // Scripts must return true or false
        Ok(self.pop_bool()?)
    }

    fn consume_op_until<F>(&mut self, mut filter: F) -> Result<(), EvalErr>
            where F: FnMut(OpFrame) -> bool {
        loop {
            match self.consume_op()? {
                Some(op) => {
                    if filter(op) { break; }
                },
                None => {
                    return Err(self.new_err(EvalErrType::UnexpectedEOF))
                }
            }
        }

        Ok(())
    }

    fn consume_op(&mut self) -> Result<Option<OpFrame>, EvalErr> {
        if self.pos == self.byte_code.len() { return Ok(None) }
        let byte = self.byte_code[self.pos];
        self.pos += 1;

        match byte {
            o if o == Operand::PushFalse as u8 => Ok(Some(OpFrame::False)),
            o if o == Operand::PushTrue as u8 => Ok(Some(OpFrame::True)),
            o if o == Operand::PushPubKey as u8 => {
                let slice = self.pos .. self.pos + sign::PUBLICKEYBYTES;
                let key = match self.byte_code.get(slice) {
                    Some(slice) => {
                        self.pos += sign::PUBLICKEYBYTES;
                        PublicKey::from_slice(slice).unwrap()
                    },
                    None => return Err(self.new_err(EvalErrType::UnexpectedEOF))
                };
                Ok(Some(OpFrame::PubKey(key)))
            },
            o if o == Operand::OpIf as u8 => Ok(Some(OpFrame::OpIf)),
            o if o == Operand::OpElse as u8 => Ok(Some(OpFrame::OpElse)),
            o if o == Operand::OpEndIf as u8 => Ok(Some(OpFrame::OpEndIf)),
            o if o == Operand::OpReturn as u8 => Ok(Some(OpFrame::OpReturn)),
            _ => Err(self.new_err(EvalErrType::UnknownOp))
        }
    }

    fn insert(&mut self, op: OpFrame) -> Result<(), EvalErr> {
        if self.stack.len() + 1 <= MAX_FRAME_STACK {
            self.stack.push(op);
            Ok(())
        } else {
            Err(self.new_err(EvalErrType::StackOverflow))
        }
    }

    fn pop_bool(&mut self) -> Result<bool, EvalErr> {
        let frame = self.pop()?;
        match frame {
            OpFrame::False => Ok(false),
            OpFrame::True => Ok(true),
            _ => Err(self.new_err(EvalErrType::InvalidCmp))
        }
    }

    fn pop(&mut self) -> Result<OpFrame, EvalErr> {
        self.stack.pop().ok_or_else(|| {
            self.new_err(EvalErrType::StackUnderflow)
        })
    }

    fn new_err(&mut self, err: EvalErrType) -> EvalErr {
        EvalErr::new(self.pos, err)
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::KeyPair;
    use super::builder::*;
    use super::*;

    #[test]
    fn true_only_script() {
        let mut engine = ScriptEngine::from(Builder::new()
                                        .push(OpFrame::True));
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn false_only_script() {
        let mut engine = ScriptEngine::from(Builder::new()
                                        .push(OpFrame::False));
        assert!(!engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn if_script() {
        let mut engine = ScriptEngine::from(Builder::new()
                                .push(OpFrame::True)
                                .push(OpFrame::OpIf)
                                .push(OpFrame::False)
                                .push(OpFrame::OpEndIf));
        assert!(!engine.eval().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine = ScriptEngine::from(Builder::new()
                                .push(OpFrame::True)
                                .push(OpFrame::OpIf)
                                .push(OpFrame::True)
                                .push(OpFrame::OpEndIf));
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn if_script_with_ret() {
        let mut engine = ScriptEngine::from(Builder::new()
                                .push(OpFrame::True)
                                .push(OpFrame::OpIf)
                                .push(OpFrame::False)
                                .push(OpFrame::OpReturn)
                                .push(OpFrame::OpEndIf)
                                .push(OpFrame::True));
        assert!(!engine.eval().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine = ScriptEngine::from(Builder::new()
                                .push(OpFrame::False)
                                .push(OpFrame::OpIf)
                                .push(OpFrame::False)
                                .push(OpFrame::OpElse)
                                .push(OpFrame::True)
                                .push(OpFrame::OpReturn)
                                .push(OpFrame::OpEndIf)
                                .push(OpFrame::False));
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn branch_if() {
        let mut engine = ScriptEngine::from(Builder::new()
                                .push(OpFrame::True)
                                .push(OpFrame::OpIf)
                                .push(OpFrame::True)
                                .push(OpFrame::OpElse)
                                .push(OpFrame::False)
                                .push(OpFrame::OpEndIf));
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine = ScriptEngine::from(Builder::new()
                                .push(OpFrame::False)
                                .push(OpFrame::OpIf)
                                .push(OpFrame::False)
                                .push(OpFrame::OpElse)
                                .push(OpFrame::True)
                                .push(OpFrame::OpEndIf));
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn nested_branch_if() {
        let mut engine = ScriptEngine::from(Builder::new()
                                .push(OpFrame::True)
                                .push(OpFrame::OpIf)
                                    .push(OpFrame::True)
                                    .push(OpFrame::OpIf)
                                        .push(OpFrame::True)
                                    .push(OpFrame::OpEndIf)
                                .push(OpFrame::OpElse)
                                    .push(OpFrame::False)
                                    .push(OpFrame::OpIf)
                                        .push(OpFrame::False)
                                    .push(OpFrame::OpEndIf)
                                .push(OpFrame::OpEndIf));
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());

        let mut engine = ScriptEngine::from(Builder::new()
                                .push(OpFrame::False)
                                .push(OpFrame::OpIf)
                                    .push(OpFrame::True)
                                    .push(OpFrame::OpIf)
                                        .push(OpFrame::False)
                                    .push(OpFrame::OpEndIf)
                                .push(OpFrame::OpElse)
                                    .push(OpFrame::True)
                                    .push(OpFrame::OpIf)
                                        .push(OpFrame::True)
                                    .push(OpFrame::OpEndIf)
                                .push(OpFrame::OpEndIf));
        assert!(engine.eval().unwrap());
        assert!(engine.stack.is_empty());
    }

    #[test]
    fn fail_invalid_stack_on_return() {
        let key = KeyPair::gen_keypair().0;
        let mut engine = ScriptEngine::from(Builder::new()
                                        .push(OpFrame::PubKey(key)));
        assert_eq!(engine.eval().unwrap_err().err, EvalErrType::InvalidCmp);
    }

    #[test]
    fn fail_invalid_if_cmp() {
        let key = KeyPair::gen_keypair().0;
        let mut engine = ScriptEngine::from(Builder::new()
                                        .push(OpFrame::PubKey(key))
                                        .push(OpFrame::OpIf));
        assert_eq!(engine.eval().unwrap_err().err, EvalErrType::InvalidCmp);
    }

    #[test]
    fn fail_unended_if() {
        let mut engine = ScriptEngine::from(Builder::new()
                                .push(OpFrame::True)
                                .push(OpFrame::OpIf));
        assert_eq!(engine.eval().unwrap_err().err, EvalErrType::UnexpectedEOF);

        let mut engine = ScriptEngine::from(Builder::new()
                                .push(OpFrame::False)
                                .push(OpFrame::OpIf));
        assert_eq!(engine.eval().unwrap_err().err, EvalErrType::UnexpectedEOF);
    }
}
