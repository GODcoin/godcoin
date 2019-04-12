use crate::crypto::PublicKey;

use super::error::*;
use super::OpFrame;
use super::MAX_FRAME_STACK;

#[derive(Debug)]
pub struct Stack {
    inner: Vec<OpFrame>,
}

impl Stack {
    pub fn new() -> Stack {
        Stack {
            inner: Vec::with_capacity(MAX_FRAME_STACK),
        }
    }

    pub fn push<T>(&mut self, op: T) -> Result<(), EvalErrType>
    where
        T: Into<OpFrame>,
    {
        if self.inner.len() < MAX_FRAME_STACK {
            self.inner.push(op.into());
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
            _ => Err(EvalErrType::InvalidItemOnStack),
        }
    }

    pub fn pop_pubkey(&mut self) -> Result<PublicKey, EvalErrType> {
        let frame = self.pop()?;
        match frame {
            OpFrame::PubKey(key) => Ok(key),
            _ => Err(EvalErrType::InvalidItemOnStack),
        }
    }

    #[inline]
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
