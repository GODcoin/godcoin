use crate::crypto::PublicKey;

use super::constants::MAX_FRAME_STACK;
use super::error::*;
use super::OpFrame;

pub struct Stack {
    inner: Vec<OpFrame>
}

impl Stack {
    pub fn new() -> Stack {
        Stack {
            inner: Vec::with_capacity(MAX_FRAME_STACK)
        }
    }

    pub fn push(&mut self, op: OpFrame) -> Result<(), EvalErrType> {
        if self.inner.len() < MAX_FRAME_STACK {
            self.inner.push(op);
            Ok(())
        } else {
            Err(EvalErrType::StackOverflow)
        }
    }

    pub fn pop(&mut self) -> Result<OpFrame, EvalErrType> {
        self.inner.pop().ok_or(EvalErrType::StackUnderflow)
    }

    pub fn pop_bool(&mut self) -> Result<bool, EvalErrType> {
        let frame = self.pop()?;
        match frame {
            OpFrame::False => Ok(false),
            OpFrame::True => Ok(true),
            _ => Err(EvalErrType::InvalidItemOnStack)
        }
    }

    pub fn pop_pubkey(&mut self) -> Result<PublicKey, EvalErrType> {
        let frame = self.pop()?;
        match frame {
            OpFrame::PubKey(key) => Ok(key),
            _ => Err(EvalErrType::InvalidItemOnStack)
        }
    }

    #[inline]
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
